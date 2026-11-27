// SPDX-License-Identifier: GPL-3.0-or-later
//! The client's synchronised object: the user input stream (`src/statesync/user.cc`).
//!
//! A `UserStream` state is the full, append-only sequence of user events since the session
//! began. Every state ever sent is therefore a prefix of the current one, so a state is fully
//! described by its length and a diff is simply "the events after position `n`". Events that
//! the server has acknowledged are discarded.

use std::collections::VecDeque;

use prost::Message as _;

use crate::proto::client_buffers::{Instruction, Keystroke, ResizeMessage, UserMessage};

/// One user event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserEvent {
    /// A byte typed (or pasted) by the user.
    Byte(u8),
    /// The terminal was resized.
    Resize {
        /// Columns.
        width: i32,
        /// Rows.
        height: i32,
    },
}

/// The append-only input stream, storing only events not yet acknowledged.
#[derive(Debug, Default, Clone)]
pub struct UserStream {
    /// Absolute position of `events[0]`.
    base: u64,
    events: VecDeque<UserEvent>,
}

impl UserStream {
    /// Empty stream.
    pub fn new() -> Self {
        Self::default()
    }

    /// Absolute length (number of events ever pushed); identifies the current state.
    pub fn len(&self) -> u64 {
        self.base + self.events.len() as u64
    }

    /// Whether no event was ever pushed.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Events held (not yet known to the receiver).
    pub fn retained(&self) -> usize {
        self.events.len()
    }

    /// Appends typed bytes.
    pub fn push_bytes(&mut self, bytes: &[u8]) {
        self.events
            .extend(bytes.iter().copied().map(UserEvent::Byte));
    }

    /// Appends a resize.
    pub fn push_resize(&mut self, width: u16, height: u16) {
        self.events.push_back(UserEvent::Resize {
            width: i32::from(width),
            height: i32::from(height),
        });
    }

    /// Forgets events before absolute position `pos` (they are known to the receiver).
    pub fn discard_before(&mut self, pos: u64) {
        while self.base < pos && !self.events.is_empty() {
            self.events.pop_front();
            self.base += 1;
        }
    }

    /// Serialised `ClientBuffers.UserMessage` with the events after absolute position `from`,
    /// exactly as mosh's `UserStream::diff_from`: consecutive bytes are merged into one
    /// `Keystroke`, each resize is its own instruction. Empty if there is nothing new.
    pub fn diff_from(&self, from: u64) -> Vec<u8> {
        let start = usize::try_from(from.saturating_sub(self.base)).unwrap_or(usize::MAX);
        let mut out = UserMessage::default();
        for ev in self.events.iter().skip(start) {
            match *ev {
                UserEvent::Byte(b) => {
                    if let Some(Instruction {
                        keystroke: Some(Keystroke { keys: Some(keys) }),
                        ..
                    }) = out.instruction.last_mut()
                    {
                        keys.push(b);
                    } else {
                        out.instruction.push(Instruction {
                            keystroke: Some(Keystroke {
                                keys: Some(vec![b]),
                            }),
                            resize: None,
                        });
                    }
                }
                UserEvent::Resize { width, height } => out.instruction.push(Instruction {
                    keystroke: None,
                    resize: Some(ResizeMessage {
                        width: Some(width),
                        height: Some(height),
                    }),
                }),
            }
        }
        if out.instruction.is_empty() {
            Vec::new()
        } else {
            out.encode_to_vec()
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn diff_merges_keystrokes_and_splits_on_resize() {
        let mut s = UserStream::new();
        s.push_bytes(b"ab");
        s.push_resize(80, 24);
        s.push_bytes(b"c");
        assert_eq!(s.len(), 4);
        let msg = UserMessage::decode(&s.diff_from(0)[..]).unwrap();
        assert_eq!(msg.instruction.len(), 3);
        assert_eq!(
            msg.instruction[0]
                .keystroke
                .as_ref()
                .unwrap()
                .keys
                .as_deref(),
            Some(&b"ab"[..])
        );
        assert_eq!(msg.instruction[1].resize.as_ref().unwrap().width, Some(80));
        assert_eq!(
            msg.instruction[2]
                .keystroke
                .as_ref()
                .unwrap()
                .keys
                .as_deref(),
            Some(&b"c"[..])
        );
        let tail = UserMessage::decode(&s.diff_from(3)[..]).unwrap();
        assert_eq!(tail.instruction.len(), 1);
        assert!(s.diff_from(4).is_empty());
    }

    #[test]
    fn discard_keeps_absolute_positions() {
        let mut s = UserStream::new();
        s.push_bytes(b"hello");
        s.discard_before(3);
        assert_eq!(s.len(), 5);
        assert_eq!(s.retained(), 2);
        let msg = UserMessage::decode(&s.diff_from(3)[..]).unwrap();
        assert_eq!(
            msg.instruction[0]
                .keystroke
                .as_ref()
                .unwrap()
                .keys
                .as_deref(),
            Some(&b"lo"[..])
        );
        // Diffs from before the discarded prefix only contain what is retained.
        assert_eq!(s.diff_from(0), s.diff_from(3));
    }
}
