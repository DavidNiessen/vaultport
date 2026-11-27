// SPDX-License-Identifier: GPL-3.0-or-later
//! Sans-IO Telnet client codec: byte-stream decoder, encoder and option negotiation.
//!
//! Feed received bytes to [`Codec::decode`]; it returns application events and queues any
//! protocol replies, which the caller sends after [`Codec::take_transmit`]. User input goes
//! through [`Codec::encode_data`]. The decoder keeps its state between calls, so input may be
//! split at any byte boundary.

use crate::consts::{CR, DO, DONT, IAC, LF, NUL, SB, SE, TelnetOption, WILL, WONT, sub};
use crate::qmethod::{Outcome, SideState};

/// Which party performs an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// We perform it (we send WILL/WONT, the server DO/DONT).
    Local,
    /// The server performs it (it sends WILL/WONT, we DO/DONT).
    Remote,
}

/// What the Return key (a bare CR from the terminal) is sent as outside BINARY mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Newline {
    /// CR LF, the Telnet end-of-line (default; what most clients send).
    #[default]
    CrLf,
    /// CR NUL, a bare carriage return.
    CrNul,
}

/// Codec configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Reply to TERMINAL-TYPE SEND.
    pub terminal_type: String,
    /// `USER` reported via NEW-ENVIRON; `None` reports no variables.
    pub user: Option<String>,
    /// Initial window size (columns, rows) for NAWS.
    pub window_size: (u16, u16),
    /// Offer our options and request the server's at start (like PuTTY's "active" mode).
    pub active_negotiation: bool,
    /// Encoding of a bare CR from the user.
    pub newline: Newline,
    /// Maximum subnegotiation payload; longer ones are discarded.
    pub max_subnegotiation: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            terminal_type: "xterm-256color".to_owned(),
            user: None,
            window_size: (80, 24),
            active_negotiation: true,
            newline: Newline::CrLf,
            max_subnegotiation: 4096,
        }
    }
}

/// Decoder output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecEvent {
    /// Terminal data (IAC escapes removed, CR NUL folded to CR outside BINARY mode).
    Data(Vec<u8>),
    /// An option was enabled or disabled.
    OptionChanged {
        /// The option.
        option: TelnetOption,
        /// Who performs it.
        side: Side,
        /// New state.
        enabled: bool,
    },
    /// A bare Telnet command such as GA, NOP, AYT or DM.
    Command(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data,
    Iac,
    Negotiate(u8),
    SbOption,
    SbData,
    SbIac,
}

#[derive(Debug, Clone, Copy, Default)]
struct OptionState {
    local: SideState,
    remote: SideState,
}

/// The Telnet client state machine.
#[derive(Debug, Clone)]
pub struct Codec {
    config: Config,
    state: State,
    options: [OptionState; 256],
    sb_option: u8,
    sb_buf: Vec<u8>,
    sb_overflow: bool,
    pending_cr: bool,
    transmit: Vec<u8>,
    data: Vec<u8>,
    window_size: (u16, u16),
}

impl Codec {
    /// Creates a codec.
    pub fn new(config: Config) -> Self {
        let window_size = config.window_size;
        Self {
            config,
            state: State::Data,
            options: [OptionState::default(); 256],
            sb_option: 0,
            sb_buf: Vec::new(),
            sb_overflow: false,
            pending_cr: false,
            transmit: Vec::new(),
            data: Vec::new(),
            window_size,
        }
    }

    /// Queues the initial negotiation (if `active_negotiation`); call once after connecting.
    pub fn start(&mut self) {
        if !self.config.active_negotiation {
            return;
        }
        self.request(Side::Remote, TelnetOption::SUPPRESS_GO_AHEAD, true);
        self.request(Side::Local, TelnetOption::SUPPRESS_GO_AHEAD, true);
        self.request(Side::Remote, TelnetOption::ECHO, true);
        self.request(Side::Local, TelnetOption::NAWS, true);
        self.request(Side::Local, TelnetOption::TERMINAL_TYPE, true);
        if self.config.user.is_some() {
            self.request(Side::Local, TelnetOption::NEW_ENVIRON, true);
        }
    }

    /// Whether we agree to `option` being enabled on `side`.
    pub fn supports(side: Side, option: TelnetOption) -> bool {
        match side {
            Side::Local => matches!(
                option,
                TelnetOption::BINARY
                    | TelnetOption::SUPPRESS_GO_AHEAD
                    | TelnetOption::TERMINAL_TYPE
                    | TelnetOption::NAWS
                    | TelnetOption::NEW_ENVIRON
            ),
            Side::Remote => matches!(
                option,
                TelnetOption::BINARY | TelnetOption::ECHO | TelnetOption::SUPPRESS_GO_AHEAD
            ),
        }
    }

    /// Whether `option` is currently enabled on `side`.
    pub fn is_enabled(&self, side: Side, option: TelnetOption) -> bool {
        self.side(side, option).enabled()
    }

    /// Asks for `option` to be enabled or disabled on `side` (RFC 1143 rules; requests made
    /// while a negotiation is in flight are queued). Enabling an unsupported option does
    /// nothing. Returns the event if the option state changed immediately (disabling).
    pub fn request(
        &mut self,
        side: Side,
        option: TelnetOption,
        enable: bool,
    ) -> Option<CodecEvent> {
        if enable && !Self::supports(side, option) {
            return None;
        }
        let st = self.side_mut(side, option);
        let outcome = if enable {
            st.request_enable()
        } else {
            st.request_disable()
        };
        self.apply(side, option, outcome)
    }

    /// Updates the window size; sends NAWS if it is enabled.
    pub fn set_window_size(&mut self, cols: u16, rows: u16) {
        self.window_size = (cols, rows);
        if self.is_enabled(Side::Local, TelnetOption::NAWS) {
            self.send_naws();
        }
    }

    /// Encodes user data: IAC is doubled and, outside BINARY mode, a CR not followed by LF or
    /// NUL gets the configured newline suffix.
    pub fn encode_data(&mut self, data: &[u8]) {
        let binary = self.is_enabled(Side::Local, TelnetOption::BINARY);
        self.transmit.reserve(data.len());
        for (i, &b) in data.iter().enumerate() {
            match b {
                IAC => self.transmit.extend_from_slice(&[IAC, IAC]),
                CR if !binary => {
                    self.transmit.push(CR);
                    if !matches!(data.get(i + 1), Some(&(LF | NUL))) {
                        self.transmit.push(match self.config.newline {
                            Newline::CrLf => LF,
                            Newline::CrNul => NUL,
                        });
                    }
                }
                _ => self.transmit.push(b),
            }
        }
    }

    /// Sends a raw Telnet command (e.g. [`crate::consts::AYT`], [`crate::consts::IP`]).
    pub fn send_command(&mut self, command: u8) {
        self.transmit.extend_from_slice(&[IAC, command]);
    }

    /// Whether protocol output is waiting.
    pub fn has_transmit(&self) -> bool {
        !self.transmit.is_empty()
    }

    /// Takes the bytes to send to the server.
    pub fn take_transmit(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.transmit)
    }

    /// Decodes received bytes.
    pub fn decode(&mut self, input: &[u8]) -> Vec<CodecEvent> {
        let mut events = Vec::new();
        let mut i = 0;
        while i < input.len() {
            let b = input[i];
            i += 1;
            match self.state {
                State::Data => {
                    if b == IAC {
                        self.state = State::Iac;
                    } else {
                        self.push_data(b);
                    }
                }
                State::Iac => self.on_command(b, &mut events),
                State::Negotiate(verb) => {
                    self.state = State::Data;
                    let option = TelnetOption(b);
                    self.flush_data(&mut events);
                    events.extend(self.on_negotiation(verb, option));
                }
                State::SbOption => {
                    self.sb_option = b;
                    self.sb_buf.clear();
                    self.sb_overflow = false;
                    self.state = State::SbData;
                }
                State::SbData => {
                    if b == IAC {
                        self.state = State::SbIac;
                    } else {
                        self.push_sb(b);
                    }
                }
                State::SbIac => match b {
                    SE => {
                        self.state = State::Data;
                        if !self.sb_overflow {
                            self.on_subnegotiation();
                        }
                        self.sb_buf.clear();
                    }
                    IAC => {
                        self.push_sb(IAC);
                        self.state = State::SbData;
                    }
                    _ => {
                        // Protocol violation: abort the subnegotiation and treat the byte
                        // as the command following IAC.
                        self.sb_buf.clear();
                        self.on_command(b, &mut events);
                    }
                },
            }
        }
        self.flush_data(&mut events);
        events
    }

    // ------------------------------------------------------------------ internals

    fn side(&self, side: Side, option: TelnetOption) -> SideState {
        let o = &self.options[usize::from(option.0)];
        match side {
            Side::Local => o.local,
            Side::Remote => o.remote,
        }
    }

    fn side_mut(&mut self, side: Side, option: TelnetOption) -> &mut SideState {
        let o = &mut self.options[usize::from(option.0)];
        match side {
            Side::Local => &mut o.local,
            Side::Remote => &mut o.remote,
        }
    }

    fn push_data(&mut self, b: u8) {
        let binary = self.is_enabled(Side::Remote, TelnetOption::BINARY);
        if !binary && self.pending_cr && b == NUL {
            self.pending_cr = false;
            return;
        }
        self.pending_cr = !binary && b == CR;
        self.data.push(b);
    }

    fn flush_data(&mut self, events: &mut Vec<CodecEvent>) {
        if !self.data.is_empty() {
            events.push(CodecEvent::Data(std::mem::take(&mut self.data)));
        }
    }

    fn push_sb(&mut self, b: u8) {
        if self.sb_buf.len() < self.config.max_subnegotiation {
            self.sb_buf.push(b);
        } else {
            self.sb_overflow = true;
        }
    }

    fn on_command(&mut self, b: u8, events: &mut Vec<CodecEvent>) {
        self.state = State::Data;
        match b {
            IAC => self.push_data(IAC),
            WILL | WONT | DO | DONT => self.state = State::Negotiate(b),
            SB => self.state = State::SbOption,
            SE => {} // stray SE
            _ => {
                self.flush_data(events);
                events.push(CodecEvent::Command(b));
            }
        }
    }

    fn on_negotiation(&mut self, verb: u8, option: TelnetOption) -> Option<CodecEvent> {
        let (side, positive) = match verb {
            WILL => (Side::Remote, true),
            WONT => (Side::Remote, false),
            DO => (Side::Local, true),
            _ => (Side::Local, false),
        };
        let accept = Self::supports(side, option);
        let st = self.side_mut(side, option);
        let outcome = if positive {
            st.recv_positive(accept)
        } else {
            st.recv_negative()
        };
        self.apply(side, option, outcome)
    }

    fn apply(&mut self, side: Side, option: TelnetOption, o: Outcome) -> Option<CodecEvent> {
        let (pos, neg) = match side {
            Side::Local => (WILL, WONT),
            Side::Remote => (DO, DONT),
        };
        if o.send_positive {
            self.transmit.extend_from_slice(&[IAC, pos, option.0]);
        }
        if o.send_negative {
            self.transmit.extend_from_slice(&[IAC, neg, option.0]);
        }
        let enabled = o.changed?;
        if enabled && side == Side::Local && option == TelnetOption::NAWS {
            self.send_naws();
        }
        if side == Side::Remote && option == TelnetOption::BINARY {
            self.pending_cr = false;
        }
        Some(CodecEvent::OptionChanged {
            option,
            side,
            enabled,
        })
    }

    fn send_subnegotiation(&mut self, option: TelnetOption, payload: &[u8]) {
        self.transmit.extend_from_slice(&[IAC, SB, option.0]);
        for &b in payload {
            if b == IAC {
                self.transmit.push(IAC);
            }
            self.transmit.push(b);
        }
        self.transmit.extend_from_slice(&[IAC, SE]);
    }

    fn send_naws(&mut self) {
        let (cols, rows) = self.window_size;
        let mut p = [0u8; 4];
        p[..2].copy_from_slice(&cols.to_be_bytes());
        p[2..].copy_from_slice(&rows.to_be_bytes());
        self.send_subnegotiation(TelnetOption::NAWS, &p);
    }

    fn on_subnegotiation(&mut self) {
        let option = TelnetOption(self.sb_option);
        let buf = std::mem::take(&mut self.sb_buf);
        match option {
            TelnetOption::TERMINAL_TYPE
                if buf.first() == Some(&sub::SEND)
                    && self.is_enabled(Side::Local, TelnetOption::TERMINAL_TYPE) =>
            {
                let mut reply = vec![sub::IS];
                reply.extend_from_slice(self.config.terminal_type.as_bytes());
                self.send_subnegotiation(option, &reply);
            }
            TelnetOption::NEW_ENVIRON
                if buf.first() == Some(&sub::SEND)
                    && self.is_enabled(Side::Local, TelnetOption::NEW_ENVIRON) =>
            {
                let reply = self.environ_reply(&buf[1..]);
                self.send_subnegotiation(option, &reply);
            }
            _ => {}
        }
        self.sb_buf = buf;
    }

    /// Builds `IS [VAR "USER" VALUE <user>]` for a NEW-ENVIRON SEND request.
    fn environ_reply(&self, request: &[u8]) -> Vec<u8> {
        let mut reply = vec![sub::IS];
        let Some(user) = self.config.user.as_deref() else {
            return reply;
        };
        if environ_requests_user(request) {
            reply.push(sub::VAR);
            environ_escape(b"USER", &mut reply);
            reply.push(sub::VALUE);
            environ_escape(user.as_bytes(), &mut reply);
        }
        reply
    }
}

/// Parses the NEW-ENVIRON SEND list: an empty list, an empty `VAR`, or `VAR "USER"` all ask
/// for USER.
fn environ_requests_user(request: &[u8]) -> bool {
    if request.is_empty() {
        return true;
    }
    let mut i = 0;
    while i < request.len() {
        let kind = request[i];
        i += 1;
        let mut name = Vec::new();
        while i < request.len() && request[i] != sub::VAR && request[i] != sub::USERVAR {
            if request[i] == sub::ESC && i + 1 < request.len() {
                i += 1;
            }
            name.push(request[i]);
            i += 1;
        }
        if kind == sub::VAR && (name.is_empty() || name == b"USER") {
            return true;
        }
    }
    false
}

fn environ_escape(bytes: &[u8], out: &mut Vec<u8>) {
    for &b in bytes {
        if matches!(b, sub::VAR | sub::VALUE | sub::ESC | sub::USERVAR) {
            out.push(sub::ESC);
        }
        out.push(b);
    }
}

/// Doubles every IAC byte (for embedding arbitrary data in the stream).
pub fn escape_iac(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        if b == IAC {
            out.push(IAC);
        }
        out.push(b);
    }
    out
}

#[cfg(test)]
mod tests;
