// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

use mosh_proto::MoshKey;

fuzz_target!(|data: &[u8]| {
    // Authenticated decryption must reject arbitrary datagrams without panicking.
    if let Ok(key) = MoshKey::parse("AAAAAAAAAAAAAAAAAAAAAA") {
        let cipher = mosh_proto::crypto::Cipher::new(&key);
        let _ = cipher.open(data);
        let _ = mosh_proto::packet::Packet::open(&cipher, data);
    }
    // Fragment reassembly is fed by authenticated payloads; it must still be robust.
    let mut asm = mosh_proto::fragment::FragmentAssembly::new();
    for chunk in data.chunks(64) {
        if let Some(f) = mosh_proto::fragment::Fragment::decode(chunk) {
            let _ = asm.add(f);
        }
    }
});
