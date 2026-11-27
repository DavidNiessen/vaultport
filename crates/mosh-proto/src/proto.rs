// SPDX-License-Identifier: GPL-3.0-or-later
//! Hand-written protobuf messages matching mosh's `src/protobufs/*.proto` (proto2).
//!
//! mosh models user and host instructions as proto2 *extensions* of an empty `Instruction`
//! message (`extensions 2 to max`). An extension is encoded exactly like an ordinary optional
//! field with the extension's number, so they are declared here as plain optional fields.
//! All scalar fields are `Option` so that "set to zero" and "absent" stay distinguishable,
//! matching proto2 presence semantics (mosh explicitly sets every `Instruction` field).

/// `transportinstruction.proto`, package `TransportBuffers`.
pub mod transport_buffers {
    /// One SSP instruction: "to go from state `old_num` to `new_num`, apply `diff`".
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct Instruction {
        /// Must be [`crate::PROTOCOL_VERSION`].
        #[prost(uint32, optional, tag = "1")]
        pub protocol_version: Option<u32>,
        /// Reference state the diff applies to.
        #[prost(uint64, optional, tag = "2")]
        pub old_num: Option<u64>,
        /// Resulting state number (`u64::MAX` = shutdown).
        #[prost(uint64, optional, tag = "3")]
        pub new_num: Option<u64>,
        /// Newest peer state the sender has received.
        #[prost(uint64, optional, tag = "4")]
        pub ack_num: Option<u64>,
        /// Receiver may discard states older than this.
        #[prost(uint64, optional, tag = "5")]
        pub throwaway_num: Option<u64>,
        /// Serialised `UserMessage` (client) or `HostMessage` (server).
        #[prost(bytes = "vec", optional, tag = "6")]
        pub diff: Option<Vec<u8>>,
        /// Random padding (0–16 bytes) to obscure lengths.
        #[prost(bytes = "vec", optional, tag = "7")]
        pub chaff: Option<Vec<u8>>,
    }
}

/// `userinput.proto`, package `ClientBuffers`.
pub mod client_buffers {
    /// A batch of user instructions (the UserStream diff).
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct UserMessage {
        /// Instructions in order.
        #[prost(message, repeated, tag = "1")]
        pub instruction: Vec<Instruction>,
    }

    /// `Instruction` with its extensions `keystroke = 2` and `resize = 3`.
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct Instruction {
        /// Extension 2.
        #[prost(message, optional, tag = "2")]
        pub keystroke: Option<Keystroke>,
        /// Extension 3.
        #[prost(message, optional, tag = "3")]
        pub resize: Option<ResizeMessage>,
    }

    /// Raw bytes typed by the user.
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct Keystroke {
        /// Field 4.
        #[prost(bytes = "vec", optional, tag = "4")]
        pub keys: Option<Vec<u8>>,
    }

    /// Window size change.
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct ResizeMessage {
        /// Columns (field 5).
        #[prost(int32, optional, tag = "5")]
        pub width: Option<i32>,
        /// Rows (field 6).
        #[prost(int32, optional, tag = "6")]
        pub height: Option<i32>,
    }
}

/// `hostinput.proto`, package `HostBuffers`.
pub mod host_buffers {
    /// A batch of host instructions (the terminal-state diff).
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct HostMessage {
        /// Instructions in order.
        #[prost(message, repeated, tag = "1")]
        pub instruction: Vec<Instruction>,
    }

    /// `Instruction` with extensions `hostbytes = 2`, `resize = 3`, `echoack = 7`.
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct Instruction {
        /// Extension 2.
        #[prost(message, optional, tag = "2")]
        pub hostbytes: Option<HostBytes>,
        /// Extension 3.
        #[prost(message, optional, tag = "3")]
        pub resize: Option<ResizeMessage>,
        /// Extension 7.
        #[prost(message, optional, tag = "7")]
        pub echoack: Option<EchoAck>,
    }

    /// Terminal output (escape sequences transforming the old frame into the new one).
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct HostBytes {
        /// Field 4.
        #[prost(bytes = "vec", optional, tag = "4")]
        pub hoststring: Option<Vec<u8>>,
    }

    /// Framebuffer size of the new state.
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct ResizeMessage {
        /// Columns (field 5).
        #[prost(int32, optional, tag = "5")]
        pub width: Option<i32>,
        /// Rows (field 6).
        #[prost(int32, optional, tag = "6")]
        pub height: Option<i32>,
    }

    /// Acknowledges user input up to a state number (used for local-echo prediction).
    #[derive(Clone, PartialEq, Eq, prost::Message)]
    pub struct EchoAck {
        /// Field 8.
        #[prost(uint64, optional, tag = "8")]
        pub echo_ack_num: Option<u64>,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use prost::Message as _;

    #[test]
    fn transport_instruction_wire_format() {
        let inst = transport_buffers::Instruction {
            protocol_version: Some(2),
            old_num: Some(0),
            new_num: Some(1),
            ack_num: Some(0),
            throwaway_num: Some(0),
            diff: Some(vec![]),
            chaff: Some(vec![0xAA]),
        };
        let bytes = inst.encode_to_vec();
        // Every field is present, as when mosh calls the setters.
        assert_eq!(
            bytes,
            [
                0x08, 2, 0x10, 0, 0x18, 1, 0x20, 0, 0x28, 0, 0x32, 0, 0x3a, 1, 0xAA
            ]
        );
        assert_eq!(
            transport_buffers::Instruction::decode(&bytes[..]).unwrap(),
            inst
        );
    }

    #[test]
    fn user_message_extension_encoding() {
        let msg = client_buffers::UserMessage {
            instruction: vec![
                client_buffers::Instruction {
                    keystroke: Some(client_buffers::Keystroke {
                        keys: Some(b"ls".to_vec()),
                    }),
                    resize: None,
                },
                client_buffers::Instruction {
                    keystroke: None,
                    resize: Some(client_buffers::ResizeMessage {
                        width: Some(80),
                        height: Some(24),
                    }),
                },
            ],
        };
        let bytes = msg.encode_to_vec();
        assert_eq!(
            bytes,
            [
                0x0a, 6, // instruction #1, len 6
                0x12, 4, // keystroke ext (field 2), len 4
                0x22, 2, b'l', b's', // keys (field 4)
                0x0a, 6, // instruction #2
                0x1a, 4, // resize ext (field 3)
                0x28, 80, 0x30, 24, // width (5), height (6)
            ]
        );
        assert_eq!(
            client_buffers::UserMessage::decode(&bytes[..]).unwrap(),
            msg
        );
    }

    #[test]
    fn host_message_decoding_with_unknown_extension() {
        let bytes = [
            0x0a, 4, 0x3a, 2, 0x40, 9, // echoack(7) { echo_ack_num(8) = 9 }
            0x0a, 6, 0x1a, 4, 0x28, 100, 0x30, 30, // resize 100x30
            0x0a, 7, 0x12, 3, 0x22, 1, b'X', 0x48, 1, // hostbytes "X" + unknown field 9
        ];
        let msg = host_buffers::HostMessage::decode(&bytes[..]).unwrap();
        assert_eq!(msg.instruction.len(), 3);
        assert_eq!(
            msg.instruction[0].echoack.as_ref().unwrap().echo_ack_num,
            Some(9)
        );
        let r = msg.instruction[1].resize.as_ref().unwrap();
        assert_eq!((r.width, r.height), (Some(100), Some(30)));
        assert_eq!(
            msg.instruction[2]
                .hostbytes
                .as_ref()
                .unwrap()
                .hoststring
                .as_deref(),
            Some(&b"X"[..])
        );
    }
}
