// SPDX-License-Identifier: GPL-3.0-or-later
//! Sans-IO client transport: network layer (`network.cc`) and the State Synchronisation
//! Protocol sender/receiver (`transportsender.cc`, `networktransport.cc`).
//!
//! # Driving the state machine
//!
//! ```text
//! loop {
//!     while let Some(d) = t.poll_transmit(now) { socket.send(d) }
//!     while let Some(e) = t.poll_event() { handle(e) }
//!     wait until t.next_timeout(), a datagram (t.on_datagram), or user input
//!     (t.push_keystrokes / t.push_resize / t.start_shutdown)
//! }
//! ```
//!
//! After *any* input call, call [`Transport::poll_transmit`] until it returns `None`; that also
//! refreshes [`Transport::next_timeout`].
//!
//! # Receiver model
//!
//! mosh-server synchronises a terminal framebuffer: each `HostBytes` diff is a string of
//! escape sequences turning the framebuffer of state `old_num` into that of `new_num`. This
//! client does not keep framebuffers; it keeps exactly one state (what the attached terminal
//! emulator currently shows) and accepts only diffs whose `old_num` is that state. Others are
//! ignored, and the current state is re-acknowledged promptly so that the server rebases its
//! diffs on it. Data-bearing states are acknowledged without the 100 ms ACK_DELAY to keep the
//! window in which the server diffs from an older state short.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use prost::Message as _;

use crate::crypto::{Cipher, OVERHEAD, SEQUENCE_MASK};
use crate::fragment::{Fragment, FragmentAssembly, Fragmenter, decode_instruction};
use crate::key::MoshKey;
use crate::packet::{self, Direction, Packet, TIMESTAMP_NONE};
use crate::proto::host_buffers::HostMessage;
use crate::proto::transport_buffers::Instruction;
use crate::user::UserStream;

/// SSP protocol version spoken by mosh 1.2–1.4.
pub const PROTOCOL_VERSION: u32 = 2;

/// mosh's default datagram size budget (`DEFAULT_SEND_MTU`).
pub const DEFAULT_MTU: usize = 500;
/// Network-layer bytes added to each fragment: 8 (nonce/seq) + 4 (timestamps).
pub const NETWORK_OVERHEAD: usize = 12;
/// Lower bound for the retransmission timeout.
pub const MIN_RTO: Duration = Duration::from_millis(50);
/// Upper bound for the retransmission timeout.
pub const MAX_RTO: Duration = Duration::from_secs(1);
/// Minimum spacing of new-state transmissions.
pub const SEND_INTERVAL_MIN: Duration = Duration::from_millis(20);
/// Maximum spacing of new-state transmissions.
pub const SEND_INTERVAL_MAX: Duration = Duration::from_millis(250);
/// Heartbeat (empty ack) interval.
pub const ACK_INTERVAL: Duration = Duration::from_secs(3);
/// Delay before acknowledging a data-free state.
pub const ACK_DELAY: Duration = Duration::from_millis(100);
/// Delay used to collect keystrokes into one packet.
pub const SEND_MINDELAY: Duration = Duration::from_millis(8);
/// Stop retransmitting if the server has been silent this long.
pub const ACTIVE_RETRY_TIMEOUT: Duration = Duration::from_secs(10);
/// Number of shutdown attempts before giving up.
pub const SHUTDOWN_RETRIES: u32 = 16;
/// Pick a new local port after this long without a round trip.
pub const PORT_HOP_INTERVAL: Duration = Duration::from_secs(10);
/// Maximum number of sent states remembered (mosh's queue limit).
const MAX_SENT_STATES: usize = 32;
/// Width of the anti-replay window, in packets.
const REPLAY_WINDOW: u64 = 128;
/// Maximum random chaff length.
const CHAFF_MAX: usize = 16;
/// Reported RTT samples above this are ignored (ms).
const RTT_SAMPLE_MAX: f64 = 5000.0;

/// Tunables.
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// Datagram size budget (including crypto and network overhead).
    pub mtu: usize,
    /// Delay before acknowledging a state that carried terminal output.
    pub data_ack_delay: Duration,
    /// Maximum unacknowledged user input events; [`Transport::push_keystrokes`] accepts only
    /// up to this many and the caller retries after acknowledgements arrive.
    pub max_unacked_input: usize,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            mtu: DEFAULT_MTU,
            data_ack_delay: Duration::ZERO,
            max_unacked_input: 256 * 1024,
        }
    }
}

/// Why a session ended.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CloseReason {
    /// The server shut the session down (shell exited) and we acknowledged it.
    ServerExited,
    /// Our shutdown request was acknowledged by the server.
    LocalShutdown,
    /// Our shutdown request was not acknowledged in time.
    ShutdownTimedOut,
}

impl std::fmt::Display for CloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ServerExited => f.write_str("server exited"),
            Self::LocalShutdown => f.write_str("session closed"),
            Self::ShutdownTimedOut => f.write_str("server did not acknowledge shutdown"),
        }
    }
}

/// Output of the state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportEvent {
    /// First authentic datagram from the server.
    Connected,
    /// Terminal output to feed to the emulator, in order.
    Output(Vec<u8>),
    /// The server framebuffer now has this size.
    RemoteResize {
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
    /// Terminal state; no more datagrams or events follow.
    Closed(CloseReason),
}

/// Counters of dropped datagrams (all drops are silent on the wire).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Datagrams received.
    pub datagrams: u64,
    /// Failed authentication or too short.
    pub auth_failures: u64,
    /// Authentic but sent in the client → server direction.
    pub wrong_direction: u64,
    /// Duplicates or older than the replay window.
    pub replays: u64,
    /// Malformed fragments or instructions (after authentication).
    pub malformed: u64,
    /// Instructions not applicable to the current state.
    pub ignored_instructions: u64,
}

#[derive(Debug, Clone, Copy)]
struct SentState {
    timestamp: Instant,
    num: u64,
    /// Length of the user stream in this state.
    len: u64,
}

/// Sliding-window replay filter over 63-bit sequence numbers.
#[derive(Debug, Default, Clone)]
struct ReplayWindow {
    highest: Option<u64>,
    /// Bit `i` set: `highest - i` has been seen.
    seen: u128,
}

impl ReplayWindow {
    /// `None` = reject; `Some(true)` = newest so far; `Some(false)` = accepted, out of order.
    fn check(&mut self, seq: u64) -> Option<bool> {
        let Some(h) = self.highest else {
            self.highest = Some(seq);
            self.seen = 1;
            return Some(true);
        };
        if seq > h {
            let shift = seq - h;
            self.seen = if shift >= REPLAY_WINDOW {
                0
            } else {
                self.seen << shift
            };
            self.seen |= 1;
            self.highest = Some(seq);
            return Some(true);
        }
        let d = h - seq;
        if d >= REPLAY_WINDOW || self.seen & (1u128 << d) != 0 {
            return None;
        }
        self.seen |= 1u128 << d;
        Some(false)
    }
}

/// The client side of a Mosh connection, without I/O.
#[derive(Debug)]
pub struct Transport {
    cipher: Cipher,
    config: TransportConfig,
    epoch: Instant,

    // --- network layer ---
    next_seq: u64,
    replay: ReplayWindow,
    srtt: f64,
    rttvar: f64,
    rtt_hit: bool,
    saved_timestamp: Option<(u16, Instant)>,
    last_heard: Option<Instant>,
    last_roundtrip_success: Instant,
    last_port_choice: Instant,

    // --- SSP sender (UserStream) ---
    user: UserStream,
    sent_states: VecDeque<SentState>,
    assumed: usize,
    next_ack_time: Instant,
    next_send_time: Option<Instant>,
    mindelay_clock: Option<Instant>,
    pending_data_ack: bool,
    remote_heard: Option<Instant>,
    ack_num: u64,
    shutdown_start: Option<Instant>,
    shutdown_tries: u32,
    fragmenter: Fragmenter,
    last_stale_reack: Option<Instant>,

    // --- SSP receiver (terminal) ---
    assembly: FragmentAssembly,
    remote_num: u64,
    connected: bool,

    transmit: VecDeque<Vec<u8>>,
    events: VecDeque<TransportEvent>,
    closed: bool,
    stats: Stats,
}

impl Transport {
    /// Creates a transport for a session key at time `now`.
    pub fn new(key: &MoshKey, config: TransportConfig, now: Instant) -> Self {
        Self {
            cipher: Cipher::new(key),
            config,
            epoch: now,
            next_seq: 0,
            replay: ReplayWindow::default(),
            srtt: 1000.0,
            rttvar: 500.0,
            rtt_hit: false,
            saved_timestamp: None,
            last_heard: None,
            last_roundtrip_success: now,
            last_port_choice: now,
            user: UserStream::new(),
            sent_states: VecDeque::from([SentState {
                timestamp: now,
                num: 0,
                len: 0,
            }]),
            assumed: 0,
            next_ack_time: now,
            next_send_time: Some(now),
            mindelay_clock: None,
            pending_data_ack: false,
            remote_heard: None,
            ack_num: 0,
            shutdown_start: None,
            shutdown_tries: 0,
            fragmenter: Fragmenter::new(),
            last_stale_reack: None,
            assembly: FragmentAssembly::new(),
            remote_num: 0,
            connected: false,
            transmit: VecDeque::new(),
            events: VecDeque::new(),
            closed: false,
            stats: Stats::default(),
        }
    }

    // ------------------------------------------------------------------ public API

    /// Queues typed bytes. Returns how many were accepted (fewer than `data.len()` when the
    /// unacknowledged-input limit is reached). Returns 0 once shutting down.
    pub fn push_keystrokes(&mut self, data: &[u8]) -> usize {
        if self.closed || self.shutdown_start.is_some() {
            return 0;
        }
        let room = self
            .config
            .max_unacked_input
            .saturating_sub(self.user.retained());
        let n = room.min(data.len());
        self.user.push_bytes(&data[..n]);
        n
    }

    /// Queues a terminal resize.
    pub fn push_resize(&mut self, cols: u16, rows: u16) {
        if self.closed || self.shutdown_start.is_some() {
            return;
        }
        self.user.push_resize(cols, rows);
    }

    /// Begins a graceful shutdown (state number `u64::MAX`).
    pub fn start_shutdown(&mut self, now: Instant) {
        if self.shutdown_start.is_none() && !self.closed {
            self.shutdown_start = Some(now);
        }
    }

    /// Processes one received datagram. Unauthentic, replayed, wrong-direction or malformed
    /// datagrams are dropped silently (see [`Transport::stats`]).
    pub fn on_datagram(&mut self, wire: &[u8], now: Instant) {
        if self.closed {
            return;
        }
        self.stats.datagrams += 1;
        let Ok(pkt) = Packet::open(&self.cipher, wire) else {
            self.stats.auth_failures += 1;
            return;
        };
        if pkt.direction != Direction::ToClient {
            self.stats.wrong_direction += 1;
            return;
        }
        let Some(newest) = self.replay.check(pkt.seq) else {
            self.stats.replays += 1;
            return;
        };
        if newest {
            self.update_timing(&pkt, now);
        }
        if !self.connected {
            self.connected = true;
            self.events.push_back(TransportEvent::Connected);
        }
        let Some(frag) = Fragment::decode(&pkt.payload) else {
            self.stats.malformed += 1;
            return;
        };
        let Some(assembled) = self.assembly.add(frag) else {
            return;
        };
        match decode_instruction(&assembled) {
            Ok(inst) => self.process_instruction(&inst, now),
            Err(e) => {
                tracing::debug!("dropping instruction: {e}");
                self.stats.malformed += 1;
            }
        }
    }

    /// Returns the next datagram to send, running the sender's timers at `now`.
    pub fn poll_transmit(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.transmit.is_empty() && !self.closed {
            self.tick(now);
            self.check_shutdown_timeout(now);
            self.calculate_timers(now);
        }
        self.transmit.pop_front()
    }

    /// Next event for the application.
    pub fn poll_event(&mut self) -> Option<TransportEvent> {
        self.events.pop_front()
    }

    /// When [`Transport::poll_transmit`] should next be called (absent other input).
    pub fn next_timeout(&self) -> Option<Instant> {
        if self.closed {
            return None;
        }
        Some(match self.next_send_time {
            Some(s) => s.min(self.next_ack_time),
            None => self.next_ack_time,
        })
    }

    /// Whether the driver should move to a new local UDP port (no round trip for
    /// [`PORT_HOP_INTERVAL`] since the last port choice). Call [`Transport::on_rebind`] after.
    pub fn needs_rebind(&self, now: Instant) -> bool {
        !self.closed
            && now.saturating_duration_since(self.last_port_choice) > PORT_HOP_INTERVAL
            && now.saturating_duration_since(self.last_roundtrip_success) > PORT_HOP_INTERVAL
    }

    /// Records that the driver switched to a new local socket. The server follows the new
    /// address as soon as it receives an authentic, newer datagram from it.
    pub fn on_rebind(&mut self, now: Instant) {
        self.last_port_choice = now;
    }

    /// Time of the newest authentic datagram from the server.
    pub fn last_heard(&self) -> Option<Instant> {
        self.last_heard
    }

    /// Smoothed round-trip time.
    pub fn srtt(&self) -> Duration {
        Duration::from_secs_f64(self.srtt / 1000.0)
    }

    /// Current retransmission timeout (`SRTT + 4·RTTVAR`, clamped to 50–1000 ms).
    pub fn rto(&self) -> Duration {
        let rto = (self.srtt + 4.0 * self.rttvar).ceil();
        Duration::from_millis(rto as u64).clamp(MIN_RTO, MAX_RTO)
    }

    /// Number of the remote (terminal) state currently applied.
    pub fn remote_state_num(&self) -> u64 {
        self.remote_num
    }

    /// Whether the session has ended.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Whether a shutdown has been started.
    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_start.is_some()
    }

    /// Unacknowledged user input events.
    pub fn unacked_input(&self) -> usize {
        self.user.retained()
    }

    /// Drop counters.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    // ------------------------------------------------------------------ network layer

    fn ms(&self, t: Instant) -> u64 {
        t.saturating_duration_since(self.epoch).as_millis() as u64
    }

    fn update_timing(&mut self, pkt: &Packet, now: Instant) {
        if pkt.timestamp != TIMESTAMP_NONE {
            self.saved_timestamp = Some((pkt.timestamp, now));
        }
        if pkt.timestamp_reply != TIMESTAMP_NONE {
            let now16 = packet::timestamp16(self.ms(now));
            let r = f64::from(packet::timestamp_diff(now16, pkt.timestamp_reply));
            if r < RTT_SAMPLE_MAX {
                if self.rtt_hit {
                    const ALPHA: f64 = 1.0 / 8.0;
                    const BETA: f64 = 1.0 / 4.0;
                    self.rttvar = (1.0 - BETA) * self.rttvar + BETA * (self.srtt - r).abs();
                    self.srtt = (1.0 - ALPHA) * self.srtt + ALPHA * r;
                } else {
                    self.srtt = r;
                    self.rttvar = r / 2.0;
                    self.rtt_hit = true;
                }
            }
        }
        self.last_heard = Some(now);
    }

    fn seal(&mut self, payload: Vec<u8>, now: Instant) {
        if self.next_seq > SEQUENCE_MASK {
            // 2^63 packets: never happens in practice; refuse to reuse a nonce.
            return;
        }
        let mut reply = TIMESTAMP_NONE;
        if let Some((ts, at)) = self.saved_timestamp {
            let held = now.saturating_duration_since(at);
            if held < Duration::from_secs(1) {
                reply = ts.wrapping_add(held.as_millis() as u16);
                self.saved_timestamp = None;
            }
        }
        let pkt = Packet {
            direction: Direction::ToServer,
            seq: self.next_seq,
            timestamp: packet::timestamp16(self.ms(now)),
            timestamp_reply: reply,
            payload,
        };
        self.next_seq += 1;
        self.transmit.push_back(pkt.seal(&self.cipher));
    }

    // ------------------------------------------------------------------ SSP receiver

    fn process_instruction(&mut self, inst: &Instruction, now: Instant) {
        if inst.protocol_version != Some(PROTOCOL_VERSION) {
            tracing::warn!(
                version = ?inst.protocol_version,
                "mosh protocol version mismatch; ignoring instruction"
            );
            self.stats.malformed += 1;
            return;
        }
        // Any authentic, well-formed instruction shows the server is alive (mosh only counts
        // applied states; we also ignore some diffs by design, see the module docs).
        self.remote_heard = Some(now);
        self.process_ack(inst.ack_num.unwrap_or(0));
        if let Some(front) = self.sent_states.front() {
            self.last_roundtrip_success = self.last_roundtrip_success.max(front.timestamp);
        }
        if self.shutdown_acknowledged() {
            self.finish(CloseReason::LocalShutdown);
            return;
        }

        let old = inst.old_num.unwrap_or(0);
        let new = inst.new_num.unwrap_or(0);
        if self.remote_num == u64::MAX || new <= self.remote_num {
            // Already have it (a retransmission): our ack may have been lost.
            self.stats.ignored_instructions += 1;
            self.maybe_reack(now);
            return;
        }
        let shutdown = new == u64::MAX;
        if old != self.remote_num {
            self.stats.ignored_instructions += 1;
            if !shutdown {
                if old < self.remote_num {
                    // The server diffs from a state we have moved past.
                    self.maybe_reack(now);
                }
                return;
            }
        }
        let diff = inst.diff.as_deref().unwrap_or_default();
        if old == self.remote_num && !diff.is_empty() {
            let Ok(msg) = HostMessage::decode(diff) else {
                self.stats.malformed += 1;
                return;
            };
            for hi in msg.instruction {
                if let Some(r) = hi.resize {
                    let clamp =
                        |v: Option<i32>| u16::try_from(v.unwrap_or(0).max(0)).unwrap_or(u16::MAX);
                    self.events.push_back(TransportEvent::RemoteResize {
                        cols: clamp(r.width),
                        rows: clamp(r.height),
                    });
                }
                if let Some(bytes) = hi.hostbytes.and_then(|h| h.hoststring)
                    && !bytes.is_empty()
                {
                    self.events.push_back(TransportEvent::Output(bytes));
                }
            }
            self.pending_data_ack = true;
        }
        self.remote_num = new;
        self.ack_num = new;
    }

    fn maybe_reack(&mut self, now: Instant) {
        let due = self
            .last_stale_reack
            .is_none_or(|t| now.saturating_duration_since(t) >= self.rto());
        if due {
            self.last_stale_reack = Some(now);
            self.pending_data_ack = true;
        }
    }

    // ------------------------------------------------------------------ SSP sender

    fn process_ack(&mut self, ack_num: u64) {
        let Some(pos) = self.sent_states.iter().position(|s| s.num == ack_num) else {
            return;
        };
        self.sent_states.drain(..pos);
        self.assumed = self.assumed.saturating_sub(pos);
    }

    fn shutdown_acknowledged(&self) -> bool {
        self.sent_states.front().is_some_and(|s| s.num == u64::MAX)
    }

    fn send_interval(&self) -> Duration {
        let ms = (self.srtt / 2.0).ceil() as u64;
        Duration::from_millis(ms).clamp(SEND_INTERVAL_MIN, SEND_INTERVAL_MAX)
    }

    fn back(&self) -> SentState {
        *self.sent_states.back().expect("sent_states is never empty")
    }

    fn front(&self) -> SentState {
        *self
            .sent_states
            .front()
            .expect("sent_states is never empty")
    }

    fn heard_recently(&self, now: Instant) -> bool {
        self.remote_heard
            .is_some_and(|h| h + ACTIVE_RETRY_TIMEOUT > now)
    }

    fn update_assumed_receiver_state(&mut self, now: Instant) {
        let window = self.rto() + ACK_DELAY;
        self.assumed = 0;
        for (i, s) in self.sent_states.iter().enumerate().skip(1) {
            if now.saturating_duration_since(s.timestamp) < window {
                self.assumed = i;
            } else {
                break;
            }
        }
    }

    fn calculate_timers(&mut self, now: Instant) {
        self.update_assumed_receiver_state(now);
        // rationalize_states: everything up to the acknowledged state is known remotely.
        self.user.discard_before(self.front().len);

        let ack_delay = self.config.data_ack_delay.min(ACK_DELAY);
        if self.pending_data_ack && self.next_ack_time > now + ack_delay {
            self.next_ack_time = now + ack_delay;
        }
        let back = self.back();
        let current = self.user.len();
        if current != back.len {
            let m = *self.mindelay_clock.get_or_insert(now);
            self.next_send_time =
                Some((m + SEND_MINDELAY).max(back.timestamp + self.send_interval()));
        } else if current != self.sent_states[self.assumed].len && self.heard_recently(now) {
            let mut t = back.timestamp + self.send_interval();
            if let Some(m) = self.mindelay_clock {
                t = t.max(m + SEND_MINDELAY);
            }
            self.next_send_time = Some(t);
        } else if current != self.front().len && self.heard_recently(now) {
            self.next_send_time = Some(back.timestamp + self.rto() + ACK_DELAY);
        } else {
            self.next_send_time = None;
        }
        if self.shutdown_start.is_some() || self.ack_num == u64::MAX {
            self.next_ack_time = back.timestamp + self.send_interval();
        }
    }

    fn tick(&mut self, now: Instant) {
        self.calculate_timers(now);
        let send_due = self.next_send_time.is_some_and(|t| now >= t);
        let ack_due = now >= self.next_ack_time;
        if !send_due && !ack_due {
            return;
        }
        let mut diff = self.user.diff_from(self.sent_states[self.assumed].len);
        self.attempt_prospective_resend_optimization(&mut diff);
        if diff.is_empty() {
            if ack_due {
                self.send_empty_ack(now);
                self.mindelay_clock = None;
            }
            if send_due {
                self.next_send_time = None;
                self.mindelay_clock = None;
            }
        } else {
            self.send_to_receiver(diff, now);
            self.mindelay_clock = None;
        }
    }

    fn attempt_prospective_resend_optimization(&mut self, diff: &mut Vec<u8>) {
        if self.assumed == 0 {
            return;
        }
        let resend = self.user.diff_from(self.front().len);
        if resend.len() <= diff.len() || (resend.len() < 1000 && resend.len() - diff.len() < 100) {
            self.assumed = 0;
            *diff = resend;
        }
    }

    fn add_sent_state(&mut self, now: Instant, num: u64) {
        self.sent_states.push_back(SentState {
            timestamp: now,
            num,
            len: self.user.len(),
        });
        if self.sent_states.len() > MAX_SENT_STATES {
            // Erase a state from the middle of the queue, like mosh.
            let idx = self.sent_states.len() - 17;
            self.sent_states.remove(idx);
            if self.assumed >= idx {
                self.assumed = self.assumed.saturating_sub(1);
            }
        }
    }

    fn send_empty_ack(&mut self, now: Instant) {
        let new_num = if self.shutdown_start.is_some() {
            u64::MAX
        } else {
            self.back().num.saturating_add(1)
        };
        self.add_sent_state(now, new_num);
        self.send_in_fragments(Vec::new(), new_num, now);
        self.next_ack_time = now + ACK_INTERVAL;
        self.next_send_time = None;
    }

    fn send_to_receiver(&mut self, diff: Vec<u8>, now: Instant) {
        let back = self.back();
        let mut new_num = if self.user.len() == back.len {
            back.num
        } else {
            back.num.saturating_add(1)
        };
        if self.shutdown_start.is_some() {
            new_num = u64::MAX;
        }
        if new_num == back.num {
            if let Some(b) = self.sent_states.back_mut() {
                b.timestamp = now;
            }
        } else {
            self.add_sent_state(now, new_num);
        }
        self.send_in_fragments(diff, new_num, now);
        self.assumed = self.sent_states.len() - 1;
        self.next_ack_time = now + ACK_INTERVAL;
        self.next_send_time = None;
    }

    fn send_in_fragments(&mut self, diff: Vec<u8>, new_num: u64, now: Instant) {
        let inst = Instruction {
            protocol_version: Some(PROTOCOL_VERSION),
            old_num: Some(self.sent_states[self.assumed].num),
            new_num: Some(new_num),
            ack_num: Some(self.ack_num),
            throwaway_num: Some(self.front().num),
            diff: Some(diff),
            chaff: Some(make_chaff()),
        };
        if new_num == u64::MAX {
            self.shutdown_tries += 1;
        }
        let mtu = self
            .config
            .mtu
            .saturating_sub(NETWORK_OVERHEAD + OVERHEAD)
            .max(crate::fragment::HEADER_LEN + 1);
        for frag in self.fragmenter.make_fragments(&inst, mtu) {
            self.seal(frag.encode(), now);
        }
        self.pending_data_ack = false;
        if self.ack_num == u64::MAX {
            // We acknowledged the server's shutdown: the session is over.
            self.finish(CloseReason::ServerExited);
        }
    }

    fn check_shutdown_timeout(&mut self, now: Instant) {
        if self.closed {
            return;
        }
        if let Some(start) = self.shutdown_start
            && (self.shutdown_tries >= SHUTDOWN_RETRIES
                || now.saturating_duration_since(start) >= ACTIVE_RETRY_TIMEOUT)
        {
            self.finish(CloseReason::ShutdownTimedOut);
        }
    }

    fn finish(&mut self, reason: CloseReason) {
        if !self.closed {
            self.closed = true;
            self.events.push_back(TransportEvent::Closed(reason));
        }
    }
}

fn make_chaff() -> Vec<u8> {
    let mut buf = [0u8; CHAFF_MAX + 1];
    // Chaff only obscures lengths; fall back to none if the OS RNG is unavailable.
    if getrandom::fill(&mut buf).is_err() {
        return Vec::new();
    }
    let len = usize::from(buf[0]) % (CHAFF_MAX + 1);
    buf[1..=len].to_vec()
}

#[cfg(test)]
mod tests;
