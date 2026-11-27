// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

use vault::crdt::{Change, Doc, diff, flatten};
use vault::hlc::{DeviceId, Hlc};

fuzz_target!(|data: &[u8]| {
    let Some((split, rest)) = data.split_first() else { return };
    let mid = usize::from(*split) % (rest.len() + 1);
    let a: Option<serde_json::Value> = serde_json::from_slice(&rest[..mid]).ok();
    let Ok(b) = serde_json::from_slice::<serde_json::Value>(&rest[mid..]) else { return };
    let _ = flatten(&b);
    let mut doc = Doc::new("x");
    for (i, c) in diff(a.as_ref(), &b).into_iter().enumerate() {
        doc.apply(Hlc { wall_ms: i as u64, counter: 0, device: DeviceId(1) }, &c);
    }
    doc.apply(Hlc { wall_ms: 0, counter: 1, device: DeviceId(2) }, &Change::Delete);
    let _ = doc.to_value();
});
