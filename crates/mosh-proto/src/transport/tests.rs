// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used)]

use super::*;
use crate::testutil::{FakeServer, host_bytes, host_resize, inst, key, user_input};

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn new_transport(t0: Instant) -> Transport {
    Transport::new(&key(), TransportConfig::default(), t0)
}

fn drain(t: &mut Transport, now: Instant) -> Vec<Vec<u8>> {
    std::iter::from_fn(|| t.poll_transmit(now)).collect()
}

fn events(t: &mut Transport) -> Vec<TransportEvent> {
    std::iter::from_fn(|| t.poll_event()).collect()
}

/// Drives the transport by its own timers from `now` until `until`.
fn advance(t: &mut Transport, mut now: Instant, until: Instant) -> Vec<(Instant, Vec<u8>)> {
    let mut out = Vec::new();
    loop {
        for d in drain(t, now) {
            out.push((now, d));
        }
        match t.next_timeout() {
            Some(next) if next <= until => now = next.max(now + ms(1)),
            _ => return out,
        }
    }
}

/// Handshake: client sends its initial resize, server answers with an empty state 1.
fn connected(t0: Instant) -> (Transport, FakeServer) {
    let mut t = new_transport(t0);
    t.push_resize(80, 24);
    let mut s = FakeServer::new();
    let first = drain(&mut t, t0);
    let i = s.receive(&first[0]).unwrap();
    let reply = s.send(
        &inst(0, 1, i.new_num.unwrap(), vec![]),
        s.last_client_timestamp,
    );
    for d in reply {
        t.on_datagram(&d, t0 + ms(50));
    }
    assert_eq!(events(&mut t), vec![TransportEvent::Connected]);
    (t, s)
}

#[test]
fn initial_packet_carries_resize() {
    let t0 = Instant::now();
    let mut t = new_transport(t0);
    t.push_resize(80, 24);
    let out = drain(&mut t, t0);
    assert_eq!(out.len(), 1);
    let i = FakeServer::new().receive(&out[0]).unwrap();
    assert_eq!(i.protocol_version, Some(2));
    assert_eq!(
        (i.old_num, i.new_num, i.ack_num, i.throwaway_num),
        (Some(0), Some(1), Some(0), Some(0))
    );
    assert!(i.chaff.unwrap().len() <= 16);
    assert_eq!(user_input(&i.diff.unwrap()).resizes, vec![(80, 24)]);
    assert!(t.next_timeout().unwrap() > t0);
}

#[test]
fn keystrokes_are_batched_and_output_is_acked() {
    let t0 = Instant::now();
    let mut t = new_transport(t0);
    t.push_resize(80, 24);
    let mut s = FakeServer::new();
    assert_eq!(drain(&mut t, t0).len(), 1);
    t.push_keystrokes(b"l");
    assert!(drain(&mut t, t0 + ms(1)).is_empty());
    t.push_keystrokes(b"s");
    let sent = advance(&mut t, t0 + ms(1), t0 + ms(500));
    assert_eq!(sent.len(), 1, "both keys in one packet");
    let i = s.receive(&sent[0].1).unwrap();
    assert_eq!(i.new_num, Some(2));
    let input = user_input(&i.diff.unwrap());
    assert_eq!(input.keys, b"ls");

    // Server applies state 2 and sends output as its state 1.
    let now = sent[0].0 + ms(30);
    for d in s.send(
        &inst(0, 1, 2, host_bytes(b"ls\r\n")),
        s.last_client_timestamp,
    ) {
        t.on_datagram(&d, now);
    }
    assert_eq!(
        events(&mut t),
        vec![
            TransportEvent::Connected,
            TransportEvent::Output(b"ls\r\n".to_vec())
        ]
    );
    assert_eq!(t.remote_state_num(), 1);
    assert_eq!(t.unacked_input(), 3);
    // Data is acknowledged immediately; acked input is released.
    let ack = drain(&mut t, now);
    assert_eq!(ack.len(), 1);
    assert_eq!(t.unacked_input(), 0);
    let a = s.receive(&ack[0]).unwrap();
    assert_eq!(a.ack_num, Some(1));
    assert_eq!(a.old_num, Some(2));
    assert_eq!(a.throwaway_num, Some(2));
    assert!(a.diff.unwrap().is_empty());
    // RTT sample from the echoed timestamp.
    assert!(t.srtt() < ms(1000));
}

#[test]
fn rtt_estimation_from_timestamp_reply() {
    let t0 = Instant::now();
    let mut t = new_transport(t0);
    t.push_resize(80, 24);
    let mut s = FakeServer::new();
    let out = drain(&mut t, t0);
    s.receive(&out[0]);
    for d in s.send(&inst(0, 1, 1, vec![]), s.last_client_timestamp) {
        t.on_datagram(&d, t0 + ms(40));
    }
    assert_eq!(t.srtt(), ms(40));
    // RTO = ceil(40 + 4 * 20) = 120 ms.
    assert_eq!(t.rto(), ms(120));
    // Clamped at MIN_RTO for tiny RTTs.
    let mut t = new_transport(t0);
    let out = drain(&mut t, t0);
    let mut s = FakeServer::new();
    s.receive(&out[0]);
    for d in s.send(&inst(0, 1, 1, vec![]), s.last_client_timestamp) {
        t.on_datagram(&d, t0 + ms(2));
    }
    assert_eq!(t.rto(), MIN_RTO);
}

#[test]
fn diffs_from_unknown_states_are_ignored() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let now = t0 + ms(100);
    for d in s.send(&inst(5, 6, 1, host_bytes(b"nope")), TIMESTAMP_NONE) {
        t.on_datagram(&d, now);
    }
    assert!(events(&mut t).is_empty());
    assert_eq!(t.stats().ignored_instructions, 1);
    for d in s.send(&inst(1, 2, 1, host_bytes(b"yes")), TIMESTAMP_NONE) {
        t.on_datagram(&d, now);
    }
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::Output(b"yes".to_vec())]
    );
    // A diff from a state we have moved past is ignored and prompts a quick re-ack.
    drain(&mut t, now);
    for d in s.send(&inst(1, 3, 1, host_bytes(b"stale")), TIMESTAMP_NONE) {
        t.on_datagram(&d, now + ms(10));
    }
    assert!(events(&mut t).is_empty());
    let reack = drain(&mut t, now + ms(10));
    assert_eq!(reack.len(), 1);
    assert_eq!(s.receive(&reack[0]).unwrap().ack_num, Some(2));
    // Retransmission of an applied state is ignored.
    for d in s.send(&inst(1, 2, 1, host_bytes(b"yes")), TIMESTAMP_NONE) {
        t.on_datagram(&d, now + ms(20));
    }
    assert!(events(&mut t).is_empty());
}

#[test]
fn replays_and_wrong_direction_and_forgeries_are_dropped() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let d = s.send(&inst(1, 2, 1, host_bytes(b"once")), TIMESTAMP_NONE);
    t.on_datagram(&d[0], t0 + ms(60));
    t.on_datagram(&d[0], t0 + ms(61));
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::Output(b"once".to_vec())]
    );
    assert_eq!(t.stats().replays, 1);

    // Client → server packet reflected back at the client.
    let mut client_pkt = drain(&mut t, t0 + ms(62));
    t.on_datagram(&client_pkt.pop().unwrap(), t0 + ms(63));
    assert_eq!(t.stats().wrong_direction, 1);
    let raw = s.seal_raw(Direction::ToServer, 1000, vec![0; 20]);
    t.on_datagram(&raw, t0 + ms(63));
    assert_eq!(t.stats().wrong_direction, 2);

    // Tampered and garbage datagrams.
    let mut bad = s
        .send(&inst(2, 3, 1, host_bytes(b"evil")), TIMESTAMP_NONE)
        .remove(0);
    let n = bad.len();
    bad[n - 1] ^= 1;
    t.on_datagram(&bad, t0 + ms(64));
    t.on_datagram(b"short", t0 + ms(64));
    t.on_datagram(&[0u8; 100], t0 + ms(64));
    assert_eq!(t.stats().auth_failures, 3);
    assert!(events(&mut t).is_empty());

    // Authentic but malformed payloads are dropped after authentication.
    t.on_datagram(
        &s.seal_raw(Direction::ToClient, 5000, vec![1, 2, 3]),
        t0 + ms(65),
    );
    assert_eq!(t.stats().malformed, 1);
}

#[test]
fn replay_window() {
    let mut w = ReplayWindow::default();
    assert_eq!(w.check(10), Some(true));
    assert_eq!(w.check(10), None);
    assert_eq!(w.check(5), Some(false));
    assert_eq!(w.check(5), None);
    assert_eq!(w.check(11), Some(true));
    assert_eq!(w.check(500), Some(true));
    assert_eq!(w.check(11), None, "older than the window");
    assert_eq!(w.check(372), None, "exactly the window width behind");
    assert_eq!(w.check(373), Some(false));
}

#[test]
fn out_of_order_fragments_of_large_output() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let mut x: u32 = 99;
    let big: Vec<u8> = (0..6000)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (x >> 16) as u8
        })
        .collect();
    let mut d = s.send_mtu(&inst(1, 2, 1, host_bytes(&big)), TIMESTAMP_NONE, 300);
    assert!(d.len() > 10);
    d.reverse();
    d.swap(2, 5);
    let dup = d[3].clone();
    for (i, p) in d.iter().enumerate() {
        t.on_datagram(p, t0 + ms(100 + i as u64));
        if i == 4 {
            t.on_datagram(&dup, t0 + ms(100));
        }
    }
    assert_eq!(events(&mut t), vec![TransportEvent::Output(big)]);
}

#[test]
fn remote_resize_event() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    for d in s.send(&inst(1, 2, 1, host_resize(132, 43)), TIMESTAMP_NONE) {
        t.on_datagram(&d, t0 + ms(70));
    }
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::RemoteResize {
            cols: 132,
            rows: 43
        }]
    );
}

#[test]
fn protocol_version_mismatch_ignored() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let mut i = inst(1, 2, 1, host_bytes(b"v3"));
    i.protocol_version = Some(3);
    for d in s.send(&i, TIMESTAMP_NONE) {
        t.on_datagram(&d, t0 + ms(70));
    }
    assert!(events(&mut t).is_empty());
    assert_eq!(t.remote_state_num(), 1);
}

#[test]
fn unacknowledged_input_is_retransmitted_after_rto() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let t1 = t0 + ms(1000);
    advance(&mut t, t0 + ms(50), t1);
    t.push_keystrokes(b"x");
    let sent = advance(&mut t, t1, t1 + ms(1500));
    let with_x: Vec<_> = sent
        .iter()
        .filter_map(|(at, d)| {
            let i = s.receive(d)?;
            (user_input(i.diff.as_deref().unwrap()).keys == b"x").then_some(*at)
        })
        .collect();
    assert!(with_x.len() >= 3, "{with_x:?}");
    let rto = t.rto();
    for w in with_x.windows(2) {
        assert!(
            w[1] - w[0] >= rto,
            "retransmitted too early: {:?}",
            w[1] - w[0]
        );
    }
}

#[test]
fn before_contact_only_heartbeat_retries() {
    let t0 = Instant::now();
    let mut t = new_transport(t0);
    t.push_resize(80, 24);
    let sent = advance(&mut t, t0, t0 + ms(10_000));
    let times: Vec<_> = sent.iter().map(|(at, _)| *at - t0).collect();
    assert_eq!(times, vec![ms(0), ms(3000), ms(6000), ms(9000)]);
    let mut s = FakeServer::new();
    for (_, d) in &sent {
        let i = s.receive(d).unwrap();
        assert_eq!(user_input(&i.diff.unwrap()).resizes, vec![(80, 24)]);
    }
}

#[test]
fn idle_heartbeats_every_ack_interval() {
    let t0 = Instant::now();
    let (mut t, _s) = connected(t0);
    let sent = advance(&mut t, t0 + ms(50), t0 + ms(9_100));
    // State 1 carried no data, so it is acknowledged by the next heartbeat only.
    assert_eq!(sent[0].0, t0 + ACK_INTERVAL);
    assert_eq!(sent.len(), 3);
    let gaps: Vec<_> = sent.windows(2).map(|w| w[1].0 - w[0].0).collect();
    assert!(gaps.iter().all(|g| *g == ACK_INTERVAL), "{gaps:?}");
}

#[test]
fn local_shutdown_acknowledged() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let now = t0 + ms(100);
    drain(&mut t, now);
    t.start_shutdown(now);
    assert_eq!(t.push_keystrokes(b"late"), 0);
    let sent = advance(&mut t, now, now + ms(60));
    assert!(sent.len() >= 2, "shutdown is retried quickly");
    let i = s.receive(&sent[0].1).unwrap();
    assert_eq!(i.new_num, Some(u64::MAX));
    for d in s.send(&inst(1, 2, u64::MAX, vec![]), TIMESTAMP_NONE) {
        t.on_datagram(&d, now + ms(70));
    }
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::Closed(CloseReason::LocalShutdown)]
    );
    assert!(t.is_closed());
    assert!(t.next_timeout().is_none());
    assert!(drain(&mut t, now + ms(500)).is_empty());
}

#[test]
fn local_shutdown_times_out() {
    let t0 = Instant::now();
    let mut t = new_transport(t0);
    t.start_shutdown(t0);
    let sent = advance(&mut t, t0, t0 + ms(20_000));
    let mut s = FakeServer::new();
    let shutdown_packets = sent
        .iter()
        .filter(|(_, d)| s.receive(d).unwrap().new_num == Some(u64::MAX))
        .count();
    assert_eq!(shutdown_packets, SHUTDOWN_RETRIES as usize);
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::Closed(CloseReason::ShutdownTimedOut)]
    );
}

#[test]
fn server_shutdown_is_acknowledged() {
    let t0 = Instant::now();
    let (mut t, mut s) = connected(t0);
    let now = t0 + ms(100);
    for d in s.send(
        &inst(1, u64::MAX, 1, host_bytes(b"logout\r\n")),
        TIMESTAMP_NONE,
    ) {
        t.on_datagram(&d, now);
    }
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::Output(b"logout\r\n".to_vec())]
    );
    let sent = advance(&mut t, now, now + ms(1000));
    assert_eq!(sent.len(), 1);
    assert_eq!(s.receive(&sent[0].1).unwrap().ack_num, Some(u64::MAX));
    assert_eq!(
        events(&mut t),
        vec![TransportEvent::Closed(CloseReason::ServerExited)]
    );
}

#[test]
fn port_hop_after_silence() {
    let t0 = Instant::now();
    let (mut t, _s) = connected(t0);
    assert!(!t.needs_rebind(t0 + ms(5000)));
    assert!(t.needs_rebind(t0 + ms(10_100)));
    t.on_rebind(t0 + ms(10_100));
    assert!(!t.needs_rebind(t0 + ms(10_200)));
    assert!(t.needs_rebind(t0 + ms(20_200)));
}

#[test]
fn unacked_input_limit() {
    let t0 = Instant::now();
    let mut t = Transport::new(
        &key(),
        TransportConfig {
            max_unacked_input: 4,
            ..TransportConfig::default()
        },
        t0,
    );
    assert_eq!(t.push_keystrokes(b"abcdef"), 4);
    assert_eq!(t.push_keystrokes(b"g"), 0);
}

mod prop {
    use super::*;
    use crate::fragment::compress;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn arbitrary_datagrams_never_panic(data in proptest::collection::vec(any::<u8>(), 0..600)) {
            let t0 = Instant::now();
            let mut t = new_transport(t0);
            t.on_datagram(&data, t0);
            prop_assert_eq!(t.stats().auth_failures, 1);
            prop_assert!(t.poll_event().is_none());
        }

        #[test]
        fn arbitrary_authentic_payloads_never_panic(
            frags in proptest::collection::vec(
                (
                    0u64..4,
                    0u16..4,
                    any::<bool>(),
                    proptest::collection::vec(any::<u8>(), 0..64),
                    any::<bool>(),
                ),
                1..12,
            )
        ) {
            let t0 = Instant::now();
            let mut t = new_transport(t0);
            let s = FakeServer::new();
            for (i, (id, num, is_final, body, zlib)) in frags.into_iter().enumerate() {
                let contents = if zlib { compress(&body) } else { body };
                let payload = Fragment { id, num, is_final, contents }.encode();
                t.on_datagram(&s.seal_raw(Direction::ToClient, i as u64, payload), t0);
                drain(&mut t, t0);
            }
            while t.poll_event().is_some() {}
        }

        #[test]
        fn arbitrary_instructions_never_panic(
            old in 0u64..4,
            new in prop_oneof![0u64..4, Just(u64::MAX)],
            ack in any::<u64>(),
            diff in proptest::collection::vec(any::<u8>(), 0..64),
        ) {
            let t0 = Instant::now();
            let (mut t, mut s) = connected(t0);
            for d in s.send(&inst(old, new, ack, diff), TIMESTAMP_NONE) {
                t.on_datagram(&d, t0 + ms(60));
            }
            advance(&mut t, t0 + ms(60), t0 + ms(20_000));
            while t.poll_event().is_some() {}
        }

        #[test]
        fn user_stream_diffs_round_trip(
            ops in proptest::collection::vec(
                prop_oneof![
                    proptest::collection::vec(any::<u8>(), 1..8).prop_map(Ok),
                    (any::<u16>(), any::<u16>()).prop_map(Err),
                ],
                0..20,
            ),
        ) {
            let mut t = new_transport(Instant::now());
            let mut keys = Vec::new();
            let mut resizes = Vec::new();
            for op in ops {
                match op {
                    Ok(k) => {
                        t.push_keystrokes(&k);
                        keys.extend(k);
                    }
                    Err((c, r)) => {
                        t.push_resize(c, r);
                        resizes.push((i32::from(c), i32::from(r)));
                    }
                }
            }
            let input = user_input(&t.user.diff_from(0));
            prop_assert_eq!(input.keys, keys);
            prop_assert_eq!(input.resizes, resizes);
        }
    }
}
