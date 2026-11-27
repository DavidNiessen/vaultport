// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

use session_log::{LogId, LogReader};

fuzz_target!(|data: &[u8]| {
    let key = vault::SymmetricKey::from_bytes([7u8; 32]);
    let id = LogId(uuid::Uuid::nil());
    if let Ok(reader) = LogReader::open_recovering(std::io::Cursor::new(data), &key, id) {
        for ev in reader.take(10_000) {
            if ev.is_err() {
                break;
            }
        }
    }
});
