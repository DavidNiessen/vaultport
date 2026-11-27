// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = importers::ssh_config::parse(s);
        let _ = importers::ssh_config::parse_with_includes(s, |_| Some(s.to_owned()));
    }
});
