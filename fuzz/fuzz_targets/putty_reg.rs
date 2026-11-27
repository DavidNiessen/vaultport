// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = importers::import(importers::Format::PuttyReg, data);
    let _ = importers::import(importers::Format::MobaXterm, data);
    let _ = importers::import(importers::Format::SecureCrtXml, data);
    let _ = importers::detect_format(data);
});
