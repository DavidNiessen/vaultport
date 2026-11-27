// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use super::*;
use crate::consts::{GA, NOP};

const TTYPE: u8 = 24;
const NAWS: u8 = 31;
const ENVIRON: u8 = 39;
const ECHO: u8 = 1;
const SGA: u8 = 3;
const BINARY: u8 = 0;

fn passive() -> Codec {
    Codec::new(Config {
        active_negotiation: false,
        ..Config::default()
    })
}

fn data(events: &[CodecEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| match e {
            CodecEvent::Data(d) => Some(d.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn changed(option: u8, side: Side, enabled: bool) -> CodecEvent {
    CodecEvent::OptionChanged {
        option: TelnetOption(option),
        side,
        enabled,
    }
}

#[test]
fn plain_data_and_escaped_iac() {
    let mut c = passive();
    let ev = c.decode(b"hi\xff\xffthere");
    assert_eq!(ev, vec![CodecEvent::Data(b"hi\xffthere".to_vec())]);
    assert!(!c.has_transmit());
}

#[test]
fn commands_split_data_in_order() {
    let mut c = passive();
    let ev = c.decode(&[b'a', b'b', IAC, NOP, b'c', IAC, GA]);
    assert_eq!(
        ev,
        vec![
            CodecEvent::Data(b"ab".to_vec()),
            CodecEvent::Command(NOP),
            CodecEvent::Data(b"c".to_vec()),
            CodecEvent::Command(GA),
        ]
    );
}

#[test]
fn accepts_server_echo_and_sga_once() {
    let mut c = passive();
    let ev = c.decode(&[IAC, WILL, ECHO, IAC, WILL, SGA]);
    assert_eq!(
        ev,
        vec![
            changed(ECHO, Side::Remote, true),
            changed(SGA, Side::Remote, true)
        ]
    );
    assert_eq!(c.take_transmit(), [IAC, DO, ECHO, IAC, DO, SGA]);
    assert!(c.is_enabled(Side::Remote, TelnetOption::ECHO));
    // A repeated WILL is not acknowledged again (no negotiation loops).
    assert!(c.decode(&[IAC, WILL, ECHO]).is_empty());
    assert!(!c.has_transmit());
    // Server stops echoing.
    let ev = c.decode(&[IAC, WONT, ECHO]);
    assert_eq!(ev, vec![changed(ECHO, Side::Remote, false)]);
    assert_eq!(c.take_transmit(), [IAC, DONT, ECHO]);
}

#[test]
fn refuses_unsupported_options() {
    let mut c = passive();
    c.decode(&[
        IAC, DO, 34, IAC, WILL, 5, IAC, DO, ECHO, IAC, WILL, TTYPE, IAC, DO, 6,
    ]);
    assert_eq!(
        c.take_transmit(),
        [
            IAC, WONT, 34, IAC, DONT, 5, IAC, WONT, ECHO, IAC, DONT, TTYPE, IAC, WONT, 6
        ]
    );
    // Refusals of already-disabled options are not answered.
    c.decode(&[IAC, DONT, 34, IAC, WONT, 5]);
    assert!(!c.has_transmit());
}

#[test]
fn terminal_type() {
    let mut c = Codec::new(Config {
        active_negotiation: false,
        terminal_type: "vt100".into(),
        ..Config::default()
    });
    // SEND before the option is enabled is ignored.
    c.decode(&[IAC, SB, TTYPE, 1, IAC, SE]);
    assert!(!c.has_transmit());
    c.decode(&[IAC, DO, TTYPE]);
    assert_eq!(c.take_transmit(), [IAC, WILL, TTYPE]);
    c.decode(&[IAC, SB, TTYPE, 1, IAC, SE]);
    let mut expected = vec![IAC, SB, TTYPE, 0];
    expected.extend_from_slice(b"vt100");
    expected.extend_from_slice(&[IAC, SE]);
    assert_eq!(c.take_transmit(), expected);
}

#[test]
fn default_terminal_type_is_xterm_256color() {
    let mut c = passive();
    c.decode(&[IAC, DO, TTYPE, IAC, SB, TTYPE, 1, IAC, SE]);
    let out = c.take_transmit();
    assert!(out.windows(14).any(|w| w == b"xterm-256color"));
}

#[test]
fn naws_on_enable_and_resize_with_escaping() {
    let mut c = passive();
    c.set_window_size(100, 40);
    assert!(!c.has_transmit(), "no NAWS before negotiation");
    c.decode(&[IAC, DO, NAWS]);
    assert_eq!(
        c.take_transmit(),
        [IAC, WILL, NAWS, IAC, SB, NAWS, 0, 100, 0, 40, IAC, SE]
    );
    c.set_window_size(255, 0x01FF);
    assert_eq!(
        c.take_transmit(),
        [IAC, SB, NAWS, 0, IAC, IAC, 1, IAC, IAC, IAC, SE]
    );
}

#[test]
fn active_negotiation() {
    let mut c = Codec::new(Config::default());
    c.start();
    assert_eq!(
        c.take_transmit(),
        [
            IAC, DO, SGA, IAC, WILL, SGA, IAC, DO, ECHO, IAC, WILL, NAWS, IAC, WILL, TTYPE
        ]
    );
    // Confirmations are not answered again; NAWS sends the size once enabled.
    let ev = c.decode(&[
        IAC, DO, NAWS, IAC, WILL, ECHO, IAC, DONT, TTYPE, IAC, WILL, SGA,
    ]);
    assert_eq!(
        ev,
        vec![
            changed(NAWS, Side::Local, true),
            changed(ECHO, Side::Remote, true),
            changed(SGA, Side::Remote, true),
        ]
    );
    assert_eq!(c.take_transmit(), [IAC, SB, NAWS, 0, 80, 0, 24, IAC, SE]);
    assert!(!c.is_enabled(Side::Local, TelnetOption::TERMINAL_TYPE));
}

#[test]
fn active_negotiation_offers_environ_only_with_user() {
    let mut c = Codec::new(Config {
        user: Some("alice".into()),
        ..Config::default()
    });
    c.start();
    assert!(c.take_transmit().ends_with(&[IAC, WILL, ENVIRON]));
}

#[test]
fn cr_nul_and_cr_lf_handling() {
    let mut c = passive();
    assert_eq!(data(&c.decode(b"a\r\0b\r\nc")), b"a\rb\r\nc");
    // CR NUL split across reads.
    assert_eq!(data(&c.decode(b"x\r")), b"x\r");
    assert_eq!(data(&c.decode(b"\0y")), b"y");
    // NUL not after CR is data.
    assert_eq!(data(&c.decode(b"\0")), b"\0");
    // In BINARY mode nothing is folded.
    c.decode(&[IAC, WILL, BINARY]);
    assert_eq!(data(&c.decode(b"\r\0")), b"\r\0");
}

#[test]
fn encoding_user_data() {
    let mut c = passive();
    c.encode_data(b"a\xffb\r");
    assert_eq!(c.take_transmit(), b"a\xff\xffb\r\n");
    c.encode_data(b"x\r\ny\r\0z\n");
    assert_eq!(c.take_transmit(), b"x\r\ny\r\0z\n");
    let mut c = Codec::new(Config {
        active_negotiation: false,
        newline: Newline::CrNul,
        ..Config::default()
    });
    c.encode_data(b"ls\r");
    assert_eq!(c.take_transmit(), b"ls\r\0");
    // Local BINARY: CR is sent raw, IAC still doubled.
    c.decode(&[IAC, DO, BINARY]);
    c.take_transmit();
    c.encode_data(b"\r\xff");
    assert_eq!(c.take_transmit(), b"\r\xff\xff");
}

#[test]
fn fragmented_input_matches_whole() {
    let mut input = vec![IAC, DO, TTYPE, b'h', b'\r'];
    input.extend_from_slice(&[
        0, IAC, IAC, IAC, SB, TTYPE, 1, IAC, SE, b'!', IAC, WILL, ECHO,
    ]);
    input.extend_from_slice(&[IAC, SB, NAWS, 1, IAC, IAC, 2, IAC, SE, b'z']);
    let mut whole = passive();
    let ev_whole = whole.decode(&input);
    let tx_whole = whole.take_transmit();

    let mut bytewise = passive();
    let mut ev_bytes = Vec::new();
    for b in &input {
        ev_bytes.extend(bytewise.decode(std::slice::from_ref(b)));
    }
    assert_eq!(data(&ev_whole), b"h\r\xff!z");
    assert_eq!(data(&ev_bytes), data(&ev_whole));
    assert_eq!(bytewise.take_transmit(), tx_whole);
    assert!(tx_whole.windows(14).any(|w| w == b"xterm-256color"));
}

#[test]
fn subnegotiation_limits_and_aborts() {
    let mut c = Codec::new(Config {
        active_negotiation: false,
        max_subnegotiation: 8,
        ..Config::default()
    });
    c.decode(&[IAC, DO, TTYPE]);
    c.take_transmit();
    // Oversized subnegotiation is discarded entirely.
    let mut big = vec![IAC, SB, TTYPE, 1];
    big.extend(std::iter::repeat_n(b'x', 100));
    big.extend_from_slice(&[IAC, SE, b'o', b'k']);
    assert_eq!(data(&c.decode(&big)), b"ok");
    assert!(!c.has_transmit());
    // IAC + non-SE inside SB aborts it and is processed as a command.
    let ev = c.decode(&[IAC, SB, TTYPE, 1, IAC, WILL, ECHO, b'q']);
    assert_eq!(
        ev,
        vec![
            changed(ECHO, Side::Remote, true),
            CodecEvent::Data(b"q".to_vec())
        ]
    );
    assert_eq!(c.take_transmit(), [IAC, DO, ECHO]);
    // Stray SE is ignored.
    assert_eq!(data(&c.decode(&[IAC, SE, b'k'])), b"k");
}

#[test]
fn new_environ_user() {
    let mut c = Codec::new(Config {
        active_negotiation: false,
        user: Some("al\u{1}ce".into()),
        ..Config::default()
    });
    c.decode(&[IAC, DO, ENVIRON]);
    assert_eq!(c.take_transmit(), [IAC, WILL, ENVIRON]);
    let mut expect = vec![IAC, SB, ENVIRON, 0, 0];
    expect.extend_from_slice(b"USER");
    expect.extend_from_slice(&[1, b'a', b'l', 2, 1, b'c', b'e', IAC, SE]);
    // Explicit request for USER.
    let mut req = vec![IAC, SB, ENVIRON, 1, 0];
    req.extend_from_slice(b"USER");
    req.extend_from_slice(&[IAC, SE]);
    c.decode(&req);
    assert_eq!(c.take_transmit(), expect);
    // "Send everything".
    c.decode(&[IAC, SB, ENVIRON, 1, IAC, SE]);
    assert_eq!(c.take_transmit(), expect);
    // Only a user variable requested: empty IS.
    let mut req = vec![IAC, SB, ENVIRON, 1, 3];
    req.extend_from_slice(b"FOO");
    req.extend_from_slice(&[IAC, SE]);
    c.decode(&req);
    assert_eq!(c.take_transmit(), [IAC, SB, ENVIRON, 0, IAC, SE]);
}

#[test]
fn new_environ_without_user_reports_nothing() {
    let mut c = passive();
    c.decode(&[IAC, DO, ENVIRON, IAC, SB, ENVIRON, 1, IAC, SE]);
    assert_eq!(
        c.take_transmit(),
        [IAC, WILL, ENVIRON, IAC, SB, ENVIRON, 0, IAC, SE]
    );
}

#[test]
fn local_requests() {
    let mut c = passive();
    assert!(
        c.request(Side::Local, TelnetOption::LINEMODE, true)
            .is_none()
    );
    assert!(!c.has_transmit(), "unsupported option is not requested");
    c.request(Side::Remote, TelnetOption::BINARY, true);
    assert_eq!(c.take_transmit(), [IAC, DO, BINARY]);
    c.decode(&[IAC, WILL, BINARY]);
    assert!(!c.has_transmit());
    assert_eq!(
        c.request(Side::Remote, TelnetOption::BINARY, false),
        Some(changed(BINARY, Side::Remote, false))
    );
    assert_eq!(c.take_transmit(), [IAC, DONT, BINARY]);
    c.decode(&[IAC, WONT, BINARY]);
    assert!(!c.has_transmit());
}

mod prop {
    use super::*;
    use proptest::prelude::*;

    fn non_data(events: &[CodecEvent]) -> Vec<CodecEvent> {
        events
            .iter()
            .filter(|e| !matches!(e, CodecEvent::Data(_)))
            .cloned()
            .collect()
    }

    proptest! {
        #[test]
        fn decoder_never_panics_and_is_chunking_independent(
            input in proptest::collection::vec(any::<u8>(), 0..512),
            cuts in proptest::collection::vec(any::<proptest::sample::Index>(), 0..8),
        ) {
            let mut whole = Codec::new(Config { max_subnegotiation: 16, ..Config::default() });
            let ev_whole = whole.decode(&input);
            let tx_whole = whole.take_transmit();

            let mut points: Vec<usize> = cuts.iter().map(|c| c.index(input.len() + 1)).collect();
            points.sort_unstable();
            let mut chunked = Codec::new(Config { max_subnegotiation: 16, ..Config::default() });
            let mut ev_chunked = Vec::new();
            let mut start = 0;
            for p in points.into_iter().chain([input.len()]) {
                ev_chunked.extend(chunked.decode(&input[start..p]));
                start = p;
            }
            prop_assert_eq!(data(&ev_chunked), data(&ev_whole));
            prop_assert_eq!(non_data(&ev_chunked), non_data(&ev_whole));
            prop_assert_eq!(chunked.take_transmit(), tx_whole);
        }

        #[test]
        fn escaped_data_round_trips(payload in proptest::collection::vec(any::<u8>(), 0..512)) {
            // Plain IAC escaping.
            let mut rx = passive();
            rx.decode(&[IAC, WILL, BINARY]);
            prop_assert_eq!(data(&rx.decode(&escape_iac(&payload))), payload.clone());

            // Encoder (local BINARY) to decoder (remote BINARY).
            let mut tx = passive();
            tx.decode(&[IAC, DO, BINARY]);
            tx.take_transmit();
            tx.encode_data(&payload);
            let wire = tx.take_transmit();
            prop_assert_eq!(data(&rx.decode(&wire)), payload);
        }

        #[test]
        fn nvt_encoding_never_leaves_bare_cr(payload in proptest::collection::vec(any::<u8>(), 0..256)) {
            let mut tx = passive();
            tx.encode_data(&payload);
            let wire = tx.take_transmit();
            for (i, b) in wire.iter().enumerate() {
                if *b == CR {
                    prop_assert!(matches!(wire.get(i + 1), Some(&(LF | NUL))));
                }
            }
        }
    }
}
