// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

// Terminal output (untrusted) split into command blocks, in arbitrary chunks.
fuzz_target!(|data: &[u8]| {
    let Some((&split, rest)) = data.split_first() else {
        return;
    };
    let mut r = term_intel::BlockRecorder::new();
    for (i, chunk) in rest.chunks(usize::from(split.max(1))).enumerate() {
        r.feed(chunk, i as i64);
    }
    let _ = r.recent(10);
    let _ = r.tail(100);
    if let Ok(s) = std::str::from_utf8(rest) {
        let comp = term_intel::Completer::new();
        for cursor in [0, s.len() / 2, s.len()] {
            let _ = comp.complete(s, cursor, 0);
        }
    }
});
