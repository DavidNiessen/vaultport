// SPDX-License-Identifier: GPL-3.0-or-later
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut codec = telnet_proto::Codec::new(telnet_proto::Config::default());
    codec.start();
    // Feed in two arbitrary splits to exercise the incremental state machine.
    let mid = data.first().map_or(0, |b| usize::from(*b) % (data.len() + 1));
    let _ = codec.decode(&data[..mid]);
    let _ = codec.decode(&data[mid..]);
    let _ = codec.take_transmit();
});
