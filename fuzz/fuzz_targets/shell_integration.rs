// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut p = term_intel::shell::ShellIntegrationParser::new();
    for chunk in data.chunks(7) {
        let _ = p.feed(chunk);
    }
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = term_intel::risk::assess_risk(s);
        let _ = term_intel::tokenize::tokenize(s);
    }
});
