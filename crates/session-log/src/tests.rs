// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::float_cmp)]

use crate::format::PREAMBLE_LEN;
use crate::*;
use std::io::Cursor;
use std::time::Duration;
use vault::SymmetricKey;

fn key() -> SymmetricKey {
    SymmetricKey::from_bytes([7u8; 32])
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn header() -> CastHeader {
    let mut h = CastHeader::new(80, 24, Some(1_700_000_000));
    h.env.insert("TERM".into(), "xterm-256color".into());
    h.title = Some("db1".into());
    h
}

/// Writes a small session with several chunks.
fn sample(id: LogId) -> Vec<u8> {
    let opts = WriterOptions {
        flush_bytes: 64,
        ..WriterOptions::default()
    };
    let mut w = LogWriter::create(Vec::new(), &key(), id, &header(), opts).unwrap();
    w.output_at(ms(0), b"\x1b[1;32muser@db1\x1b[0m:~$ ")
        .unwrap();
    w.output_at(ms(500), b"ls -la\r\n").unwrap();
    w.output_at(ms(510), b"total 0\r\ndrwxr-xr-x 2 user user 40 .\r\n")
        .unwrap();
    w.resize_at(ms(900), 120, 40).unwrap();
    w.marker_at(ms(950), "deploy started").unwrap();
    w.output_at(
        ms(1_000),
        b"\x1b[31mERROR\x1b[0m: disk \x1b[1mfull\x1b[0m on /dev/sda1\r\n",
    )
    .unwrap();
    w.output_at(ms(3_500), "caf\u{e9} \u{1F600}\r\n".as_bytes())
        .unwrap();
    let (out, summary) = w.finish().unwrap();
    assert_eq!(summary.events, 7);
    assert!(summary.chunk_count >= 3, "{summary:?}");
    out
}

/// Splits a log file into its preamble and raw records (length prefix included).
fn records(file: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let pre = file[..PREAMBLE_LEN].to_vec();
    let mut rest = &file[PREAMBLE_LEN..];
    let mut out = Vec::new();
    while !rest.is_empty() {
        let len = u32::from_le_bytes(rest[..4].try_into().unwrap()) as usize;
        out.push(rest[..4 + len].to_vec());
        rest = &rest[4 + len..];
    }
    (pre, out)
}

fn join(pre: &[u8], recs: &[Vec<u8>]) -> Vec<u8> {
    let mut v = pre.to_vec();
    for r in recs {
        v.extend_from_slice(r);
    }
    v
}

fn read_all(file: &[u8], id: LogId) -> Result<Vec<CastEvent>, LogError> {
    LogReader::open(Cursor::new(file), &key(), id)?.collect()
}

#[test]
fn roundtrip() {
    let id = LogId::new();
    let file = sample(id);
    assert_eq!(read_log_id(&mut Cursor::new(&file)).unwrap(), id);
    let mut r = LogReader::open(Cursor::new(&file), &key(), id).unwrap();
    assert_eq!(r.header(), &header());
    let events: Vec<CastEvent> = r.by_ref().collect::<Result<_, _>>().unwrap();
    assert!(r.is_complete());
    assert_eq!(events.len(), 7);
    assert_eq!(events[0].kind, EventKind::Output);
    assert_eq!(events[0].time, 0.0);
    assert_eq!(events[1].time, 0.5);
    assert_eq!(events[1].data, "ls -la\r\n");
    assert_eq!(events[3].resize_dimensions(), Some((120, 40)));
    assert_eq!(events[4].kind, EventKind::Marker);
    assert_eq!(events[4].data, "deploy started");
    assert_eq!(events[6].data, "caf\u{e9} \u{1F600}\r\n");
    assert_eq!(events[6].time, 3.5);
    let stats = LogReader::open(Cursor::new(&file), &key(), id)
        .unwrap()
        .verify()
        .unwrap();
    assert_eq!(stats.events, 7);
    assert_eq!(stats.duration, 3.5);
}

#[test]
fn export_produces_plain_asciicast() {
    let id = LogId::new();
    let file = sample(id);
    let mut cast = Vec::new();
    LogReader::open(Cursor::new(&file), &key(), id)
        .unwrap()
        .export_cast(&mut cast)
        .unwrap();
    let text = String::from_utf8(cast).unwrap();
    let mut lines = text.lines();
    let h: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(h["version"], 2);
    assert_eq!(h["width"], 80);
    let first = lines.next().unwrap();
    assert!(first.starts_with("[0.0,\"o\","), "{first}");
    assert_eq!(lines.clone().count(), 6);
    assert!(lines.any(|l| l == r#"[0.9,"r","120x40"]"#));
}

#[test]
fn bit_flips_are_detected() {
    let id = LogId::new();
    let file = sample(id);
    for pos in [PREAMBLE_LEN + 4 + 30, file.len() / 2, file.len() - 1] {
        let mut bad = file.clone();
        bad[pos] ^= 0x01;
        let err = read_all(&bad, id).unwrap_err();
        assert!(matches!(err, LogError::Tampered { .. }), "{pos}: {err:?}");
    }
    // Flipping a bit of the log id in the preamble: wrong log, or tampering if the caller
    // trusts the file's id.
    let mut bad = file.clone();
    bad[10] ^= 0x01;
    assert!(matches!(read_all(&bad, id), Err(LogError::WrongLog)));
    let other = read_log_id(&mut Cursor::new(&bad)).unwrap();
    assert!(matches!(
        read_all(&bad, other),
        Err(LogError::Tampered { chunk: 0 })
    ));
    let mut bad = file;
    bad[0] = b'X';
    assert!(matches!(read_all(&bad, id), Err(LogError::BadMagic)));
}

#[test]
fn chunk_swaps_and_foreign_chunks_are_detected() {
    let id = LogId::new();
    let file = sample(id);
    let (pre, recs) = records(&file);
    assert!(recs.len() >= 4);
    let mut swapped = recs.clone();
    swapped.swap(1, 2);
    assert!(matches!(
        read_all(&join(&pre, &swapped), id),
        Err(LogError::Tampered { chunk: 1 })
    ));
    // Dropping a middle chunk shifts the indices.
    let mut dropped = recs.clone();
    dropped.remove(2);
    assert!(matches!(
        read_all(&join(&pre, &dropped), id),
        Err(LogError::Tampered { chunk: 2 })
    ));
    // A chunk from another log with the same key.
    let other_id = LogId::new();
    let (_, other) = records(&sample(other_id));
    let mut mixed = recs.clone();
    mixed[1] = other[1].clone();
    assert!(matches!(
        read_all(&join(&pre, &mixed), id),
        Err(LogError::Tampered { chunk: 1 })
    ));
    // Wrong key.
    let wrong = SymmetricKey::from_bytes([8u8; 32]);
    assert!(matches!(
        LogReader::open(Cursor::new(&file), &wrong, id),
        Err(LogError::Tampered { chunk: 0 })
    ));
}

#[test]
fn truncation_is_detected() {
    let id = LogId::new();
    let file = sample(id);
    let (pre, recs) = records(&file);
    // Whole end chunk missing.
    let cut = join(&pre, &recs[..recs.len() - 1]);
    let events: Vec<_> = LogReader::open(Cursor::new(&cut), &key(), id)
        .unwrap()
        .collect();
    assert!(matches!(
        events.last(),
        Some(Err(LogError::Truncated { .. }))
    ));
    // Cut in the middle of a record.
    let cut = &file[..file.len() - 5];
    assert!(matches!(read_all(cut, id), Err(LogError::Truncated { .. })));
    // Data appended after the end chunk.
    let mut extra = file.clone();
    extra.extend_from_slice(&recs[1]);
    assert!(matches!(read_all(&extra, id), Err(LogError::Corrupt(_))));
    // Only the preamble.
    assert!(matches!(
        LogReader::open(Cursor::new(&pre), &key(), id),
        Err(LogError::Truncated { chunks: 0 })
    ));
}

#[test]
fn recovering_reader_reads_crashed_logs() {
    let id = LogId::new();
    let mut file = Vec::new();
    let mut w =
        LogWriter::create(&mut file, &key(), id, &header(), WriterOptions::default()).unwrap();
    w.output_at(ms(10), b"before crash\r\n").unwrap();
    w.flush().unwrap();
    w.output_at(ms(20), b"lost: never flushed\r\n").unwrap();
    drop(w); // Simulated crash: no end chunk, buffered events lost.
    let partial = file.clone();
    file.extend_from_slice(&[0x10, 0, 0, 0, 1, 2]); // torn final record
    assert!(matches!(
        read_all(&file, id),
        Err(LogError::Truncated { chunks: 2 })
    ));
    let mut r = LogReader::open_recovering(Cursor::new(&file), &key(), id).unwrap();
    let events: Vec<_> = r.by_ref().collect::<Result<_, _>>().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].data, "before crash\r\n");
    assert!(!r.is_complete());
    // Tampering is still an error in recovery mode.
    let mut bad = partial;
    let last = bad.len() - 1;
    bad[last] ^= 1;
    assert!(matches!(
        LogReader::open_recovering(Cursor::new(&bad), &key(), id)
            .unwrap()
            .verify(),
        Err(LogError::Tampered { chunk: 1 })
    ));
}

#[test]
fn search_strips_ansi() {
    let id = LogId::new();
    let file = sample(id);
    let r = LogReader::open(Cursor::new(&file), &key(), id).unwrap();
    let m = r.search("disk FULL").unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].line_excerpt, "ERROR: disk full on /dev/sda1");
    assert_eq!(m[0].time, 1.0);
    let r = LogReader::open(Cursor::new(&file), &key(), id).unwrap();
    let m = r.search("user@db1").unwrap();
    // The prompt line continues with the typed command, echoed half a second later.
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].line_excerpt, "user@db1:~$ ls -la");
    assert_eq!(m[0].time, 0.0);
    let r = LogReader::open(Cursor::new(&file), &key(), id).unwrap();
    assert_eq!(
        r.search("CAFÉ").unwrap()[0].line_excerpt,
        "caf\u{e9} \u{1F600}"
    );
    let r = LogReader::open(Cursor::new(&file), &key(), id).unwrap();
    assert!(r.search("   ").unwrap().is_empty());
    let r = LogReader::open(Cursor::new(&file), &key(), id).unwrap();
    assert!(r.search("not there").unwrap().is_empty());
}

#[test]
fn search_excerpts_long_lines() {
    let id = LogId::new();
    let mut w =
        LogWriter::create(Vec::new(), &key(), id, &header(), WriterOptions::default()).unwrap();
    let long = format!("{}NEEDLE{}\r\n", "a".repeat(500), "b".repeat(500));
    w.output_at(ms(1), long.as_bytes()).unwrap();
    // A needle split across two output events and wrapped in color codes.
    w.output_at(ms(2), b"x \x1b[31mNEE").unwrap();
    w.output_at(ms(3), b"DLE\x1b[0m y\r\n").unwrap();
    let (file, _) = w.finish().unwrap();
    let m = LogReader::open(Cursor::new(&file), &key(), id)
        .unwrap()
        .search("needle")
        .unwrap();
    assert_eq!(m.len(), 2);
    assert!(m[0].line_excerpt.starts_with('…'));
    assert!(m[0].line_excerpt.ends_with('…'));
    assert!(m[0].line_excerpt.contains("NEEDLE"));
    assert!(m[0].line_excerpt.chars().count() < 140);
    assert_eq!(m[1].line_excerpt, "x NEEDLE y");
    assert_eq!(m[1].time, 0.002);
}

#[test]
fn input_is_off_by_default_and_suppressed_when_sensitive() {
    let id = LogId::new();
    let mut w =
        LogWriter::create(Vec::new(), &key(), id, &header(), WriterOptions::default()).unwrap();
    w.input_at(ms(1), b"secret").unwrap();
    w.set_record_input(true);
    w.input_at(ms(2), b"ls\r").unwrap();
    w.set_sensitive(true);
    assert!(w.is_sensitive());
    w.input_at(ms(3), b"hunter2\r").unwrap();
    w.set_sensitive(false);
    // Automatic detection of password prompts in the output.
    w.output_at(ms(4), b"[sudo] password for alice: ").unwrap();
    assert!(w.input_suppressed());
    w.input_at(ms(5), b"sudo-secret\r").unwrap();
    w.output_at(ms(6), b"\r\n").unwrap();
    assert!(!w.input_suppressed());
    w.input_at(ms(7), b"exit\r").unwrap();
    let (file, summary) = w.finish().unwrap();
    let events = read_all(&file, id).unwrap();
    let inputs: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::Input)
        .map(|e| e.data.as_str())
        .collect();
    assert_eq!(inputs, ["ls\r", "exit\r"]);
    assert_eq!(summary.input_bytes, 8);
    let all: String = events.iter().map(|e| e.data.as_str()).collect();
    assert!(!all.contains("secret"));
    assert!(!all.contains("hunter2"));
}

#[test]
fn flush_policy_and_bounded_chunks() {
    let id = LogId::new();
    let mut w =
        LogWriter::create(Vec::new(), &key(), id, &header(), WriterOptions::default()).unwrap();
    // Small events within 2 s stay buffered.
    w.output_at(ms(0), b"a").unwrap();
    w.output_at(ms(1_000), b"b").unwrap();
    w.poll_flush_at(ms(1_500)).unwrap();
    assert_eq!(w.chunks_written(), 1);
    w.poll_flush_at(ms(2_100)).unwrap();
    assert_eq!(w.chunks_written(), 2);
    // A large burst is split into chunks of about 64 KiB.
    let big = vec![b'x'; 1024 * 1024];
    w.output_at(ms(3_000), &big).unwrap();
    assert!(w.chunks_written() >= 2 + 16);
    let (file, summary) = w.finish().unwrap();
    let (_, recs) = records(&file);
    assert!(recs.iter().all(|r| r.len() < 128 * 1024));
    assert_eq!(summary.output_bytes, 2 + 1024 * 1024);
    let stats = LogReader::open(Cursor::new(&file), &key(), id)
        .unwrap()
        .verify()
        .unwrap();
    assert_eq!(stats.output_bytes, summary.output_bytes);
    assert_eq!(stats.chunk_count, summary.chunk_count);
}

#[test]
fn utf8_split_across_writes_and_monotonic_time() {
    let id = LogId::new();
    let mut w =
        LogWriter::create(Vec::new(), &key(), id, &header(), WriterOptions::default()).unwrap();
    let bytes = "€".as_bytes();
    w.output_at(ms(5), &bytes[..1]).unwrap();
    w.output_at(ms(3), &bytes[1..]).unwrap(); // time goes backwards: clamped
    w.output_at(ms(6), b"\xff").unwrap();
    w.output_at(ms(7), &bytes[..2]).unwrap(); // incomplete at finish
    let (file, _) = w.finish().unwrap();
    let events = read_all(&file, id).unwrap();
    let data: Vec<_> = events.iter().map(|e| e.data.as_str()).collect();
    assert_eq!(data, ["€", "\u{fffd}", "\u{fffd}"]);
    assert!(events.windows(2).all(|p| p[0].time <= p[1].time));
}

#[test]
fn empty_log_is_valid() {
    // An empty log is valid.
    let id = LogId::new();
    let w = LogWriter::create(Vec::new(), &key(), id, &header(), WriterOptions::default()).unwrap();
    let (file, s) = w.finish().unwrap();
    assert_eq!(s.events, 0);
    assert_eq!(s.chunk_count, 1);
    assert!(read_all(&file, id).unwrap().is_empty());
}

mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]
        #[test]
        fn random_corruption_never_panics(pos in 0usize..4096, val in any::<u8>(), cut in 0usize..4096) {
            let id = LogId::from_uuid(uuid::Uuid::from_u128(42));
            let file = sample(id);
            let mut bad = file.clone();
            let p = pos % bad.len();
            bad[p] = val;
            let bad = &bad[..cut.min(bad.len()).max(1)];
            let _ = LogReader::open(Cursor::new(bad), &key(), id).map(LogReader::verify);
            let _ = LogReader::open_recovering(Cursor::new(bad), &key(), id).map(LogReader::verify);
        }
    }
}
