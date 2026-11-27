// SPDX-License-Identifier: GPL-3.0-or-later
//! RFC 1143 "Q method" option negotiation state for one side of one option.
//!
//! Each side of each option is in one of `NO`, `YES`, `WANTNO`, `WANTYES`, with a one-bit
//! queue (`EMPTY` / `OPPOSITE`) for a request made while a negotiation is in flight. The rules
//! guarantee that negotiation never loops, whatever the peer does.

/// Negotiation state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Q {
    /// Disabled.
    #[default]
    No,
    /// Enabled.
    Yes,
    /// We asked to disable and await the reply.
    WantNo,
    /// We asked to enable and await the reply.
    WantYes,
}

/// What the codec must do after a state transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Send the positive verb (WILL for our side, DO for the peer's side).
    pub send_positive: bool,
    /// Send the negative verb (WONT / DONT).
    pub send_negative: bool,
    /// The option became enabled (`Some(true)`) or disabled (`Some(false)`).
    pub changed: Option<bool>,
}

impl Outcome {
    const NONE: Self = Self {
        send_positive: false,
        send_negative: false,
        changed: None,
    };
    const POS: Self = Self {
        send_positive: true,
        ..Self::NONE
    };
    const NEG: Self = Self {
        send_negative: true,
        ..Self::NONE
    };
    const ENABLED: Self = Self {
        changed: Some(true),
        ..Self::NONE
    };
}

/// One side (ours or the peer's) of one option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SideState {
    /// Current state.
    pub q: Q,
    /// Queue bit: `true` = OPPOSITE.
    pub opposite: bool,
}

impl SideState {
    /// Whether the option is enabled.
    pub fn enabled(self) -> bool {
        self.q == Q::Yes
    }

    /// The peer sent the positive verb (WILL for its side, DO for ours). `accept` is our
    /// policy for enabling the option.
    pub fn recv_positive(&mut self, accept: bool) -> Outcome {
        match (self.q, self.opposite) {
            (Q::No, _) => {
                if accept {
                    self.q = Q::Yes;
                    Outcome {
                        send_positive: true,
                        changed: Some(true),
                        ..Outcome::NONE
                    }
                } else {
                    Outcome::NEG
                }
            }
            (Q::Yes, _) => Outcome::NONE,
            // Error: our DONT/WONT was answered by WILL/DO.
            (Q::WantNo, false) => {
                self.q = Q::No;
                Outcome::NONE
            }
            (Q::WantNo, true) => {
                self.q = Q::Yes;
                self.opposite = false;
                Outcome::ENABLED
            }
            (Q::WantYes, false) => {
                self.q = Q::Yes;
                Outcome::ENABLED
            }
            (Q::WantYes, true) => {
                self.q = Q::WantNo;
                self.opposite = false;
                Outcome::NEG
            }
        }
    }

    /// The peer sent the negative verb (WONT for its side, DONT for ours).
    pub fn recv_negative(&mut self) -> Outcome {
        match (self.q, self.opposite) {
            (Q::No, _) => Outcome::NONE,
            (Q::Yes, _) => {
                self.q = Q::No;
                Outcome {
                    send_negative: true,
                    changed: Some(false),
                    ..Outcome::NONE
                }
            }
            (Q::WantNo, false) | (Q::WantYes, _) => {
                self.q = Q::No;
                self.opposite = false;
                Outcome::NONE
            }
            (Q::WantNo, true) => {
                self.q = Q::WantYes;
                self.opposite = false;
                Outcome::POS
            }
        }
    }

    /// We want the option enabled.
    pub fn request_enable(&mut self) -> Outcome {
        match (self.q, self.opposite) {
            (Q::No, _) => {
                self.q = Q::WantYes;
                Outcome::POS
            }
            (Q::WantNo, false) => {
                self.opposite = true;
                Outcome::NONE
            }
            (Q::WantYes, true) => {
                self.opposite = false;
                Outcome::NONE
            }
            // Already enabled, already negotiating, or already queued.
            (Q::Yes | Q::WantNo | Q::WantYes, _) => Outcome::NONE,
        }
    }

    /// We want the option disabled.
    pub fn request_disable(&mut self) -> Outcome {
        match (self.q, self.opposite) {
            (Q::Yes, _) => {
                self.q = Q::WantNo;
                Outcome {
                    send_negative: true,
                    changed: Some(false),
                    ..Outcome::NONE
                }
            }
            (Q::WantNo, true) => {
                self.opposite = false;
                Outcome::NONE
            }
            (Q::WantYes, false) => {
                self.opposite = true;
                Outcome::NONE
            }
            (Q::No | Q::WantNo | Q::WantYes, _) => Outcome::NONE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accept_and_refuse_from_no() {
        let mut s = SideState::default();
        assert_eq!(s.recv_positive(false), Outcome::NEG);
        assert_eq!(s.q, Q::No);
        let o = s.recv_positive(true);
        assert!(o.send_positive && o.changed == Some(true));
        assert!(s.enabled());
        // Repeated WILL while YES must not be answered (prevents loops).
        assert_eq!(s.recv_positive(true), Outcome::NONE);
    }

    #[test]
    fn our_request_confirmed_or_refused() {
        let mut s = SideState::default();
        assert_eq!(s.request_enable(), Outcome::POS);
        assert_eq!(s.q, Q::WantYes);
        assert_eq!(s.recv_positive(true), Outcome::ENABLED);
        let mut s = SideState::default();
        s.request_enable();
        assert_eq!(s.recv_negative(), Outcome::NONE);
        assert_eq!(s.q, Q::No);
    }

    #[test]
    fn queued_opposite_requests() {
        // Enable then disable while waiting: on WILL we immediately send DONT.
        let mut s = SideState::default();
        s.request_enable();
        assert_eq!(s.request_disable(), Outcome::NONE);
        assert!(s.opposite);
        assert_eq!(s.recv_positive(true), Outcome::NEG);
        assert_eq!((s.q, s.opposite), (Q::WantNo, false));
        assert_eq!(s.recv_negative(), Outcome::NONE);
        assert_eq!(s.q, Q::No);

        // Disable then enable while waiting: WONT triggers a fresh DO.
        let mut s = SideState {
            q: Q::Yes,
            opposite: false,
        };
        assert!(s.request_disable().send_negative);
        assert_eq!(s.request_enable(), Outcome::NONE);
        assert_eq!(s.recv_negative(), Outcome::POS);
        assert_eq!(s.q, Q::WantYes);
        assert_eq!(s.recv_positive(true), Outcome::ENABLED);
    }

    #[test]
    fn peer_disables_enabled_option() {
        let mut s = SideState {
            q: Q::Yes,
            opposite: false,
        };
        let o = s.recv_negative();
        assert!(o.send_negative && o.changed == Some(false));
        assert_eq!(s.recv_negative(), Outcome::NONE);
    }

    #[test]
    fn wantno_answered_by_positive_is_error_but_terminates() {
        let mut s = SideState {
            q: Q::WantNo,
            opposite: false,
        };
        assert_eq!(s.recv_positive(true), Outcome::NONE);
        assert_eq!(s.q, Q::No);
        let mut s = SideState {
            q: Q::WantNo,
            opposite: true,
        };
        assert_eq!(s.recv_positive(true), Outcome::ENABLED);
    }
}
