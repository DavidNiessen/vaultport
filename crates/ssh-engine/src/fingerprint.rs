// SPDX-License-Identifier: GPL-3.0-or-later
//! `SHA256:` fingerprints and OpenSSH-compatible "randomart" (the drunken bishop).

use russh::keys::ssh_key::public::KeyData;
use russh::keys::ssh_key::{Algorithm, EcdsaCurve, HashAlg, PublicKey};

/// `SHA256:<base64>` fingerprint of a public key, identical to `ssh-keygen -l`.
#[must_use]
pub fn sha256_fingerprint(key: &KeyData) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Short key type label and size as printed by `ssh-keygen -l`, e.g. `("ED25519", 256)`.
#[must_use]
pub fn key_type_and_bits(key: &KeyData) -> (&'static str, u32) {
    match key.algorithm() {
        Algorithm::Ed25519 => ("ED25519", 256),
        Algorithm::Ecdsa { curve } => ("ECDSA", curve_bits(curve)),
        Algorithm::Rsa { .. } => (
            "RSA",
            match key {
                KeyData::Rsa(rsa) => rsa.key_size(),
                _ => 0,
            },
        ),
        Algorithm::SkEd25519 => ("ED25519-SK", 256),
        Algorithm::SkEcdsaSha2NistP256 => ("ECDSA-SK", 256),
        Algorithm::Dsa => ("DSA", 1024),
        _ => ("UNKNOWN", 0),
    }
}

const fn curve_bits(curve: EcdsaCurve) -> u32 {
    match curve {
        EcdsaCurve::NistP256 => 256,
        EcdsaCurve::NistP384 => 384,
        EcdsaCurve::NistP521 => 521,
    }
}

/// Randomart of a public key's SHA-256 fingerprint, identical to `ssh-keygen -lv`.
#[must_use]
pub fn randomart(key: &KeyData) -> String {
    let (kind, bits) = key_type_and_bits(key);
    let fp = key.fingerprint(HashAlg::Sha256);
    randomart_from_digest(fp.as_bytes(), &format!("{kind} {bits}"), "SHA256")
}

/// Convenience for a full [`PublicKey`].
#[must_use]
pub fn randomart_for(key: &PublicKey) -> String {
    randomart(key.key_data())
}

const FIELD_X: usize = 17;
const FIELD_Y: usize = 9;
const SYMBOLS: &[u8] = b" .o+=*BOX@%&#/^SE";

/// The "drunken bishop" visualisation used by OpenSSH (`sshkey_fingerprint_randomart`).
///
/// `title` is shown in the top border (e.g. `ED25519 256`), `hash_name` in the bottom border.
/// Lines are separated by `\n`, without a trailing newline.
#[must_use]
pub fn randomart_from_digest(digest: &[u8], title: &str, hash_name: &str) -> String {
    // Index of 'S' is len-1, of 'E' is len, counters saturate at len-2 ('^').
    let len = SYMBOLS.len() - 1;
    let mut field = [[0usize; FIELD_Y]; FIELD_X];
    let (mut x, mut y) = (FIELD_X / 2, FIELD_Y / 2);
    for &byte in digest {
        let mut input = byte;
        for _ in 0..4 {
            x = if input & 0x1 == 0 {
                x.saturating_sub(1)
            } else {
                (x + 1).min(FIELD_X - 1)
            };
            y = if input & 0x2 == 0 {
                y.saturating_sub(1)
            } else {
                (y + 1).min(FIELD_Y - 1)
            };
            if field[x][y] < len - 2 {
                field[x][y] += 1;
            }
            input >>= 2;
        }
    }
    field[FIELD_X / 2][FIELD_Y / 2] = len - 1;
    field[x][y] = len;

    let mut out = String::with_capacity((FIELD_X + 3) * (FIELD_Y + 2));
    border(&mut out, &format!("[{title}]"));
    out.push('\n');
    for row in 0..FIELD_Y {
        out.push('|');
        for column in &field {
            out.push(char::from(SYMBOLS[column[row].min(len)]));
        }
        out.push_str("|\n");
    }
    border(&mut out, &format!("[{hash_name}]"));
    out
}

fn border(out: &mut String, label: &str) {
    let label: String = label.chars().take(FIELD_X).collect();
    let tlen = label.chars().count();
    let left = (FIELD_X - tlen) / 2;
    out.push('+');
    out.extend(std::iter::repeat_n('-', left));
    out.push_str(&label);
    out.extend(std::iter::repeat_n('-', FIELD_X - left - tlen));
    out.push('+');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(line: &str) -> PublicKey {
        PublicKey::from_openssh(line).expect("valid test key")
    }

    // Vectors produced with `ssh-keygen -lv -f key.pub` (OpenSSH 10.0p2).
    #[test]
    fn ed25519_matches_ssh_keygen() {
        let k = key(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILczREmvKvAP+M8c6REGu9s7P7HaOXLerxerEgBNrsUH test@example",
        );
        assert_eq!(
            sha256_fingerprint(k.key_data()),
            "SHA256:lYCAwsuWE92HHmWeXQV95LR4iaQ3/BVrnJoW7iZcEQo"
        );
        let expected = "\
+--[ED25519 256]--+
|. ..o.o+.E o++.+ |
|.o.. ++.o.o.=.B.*|
|..+ . oo .oo O.X.|
| *   .   .  o X .|
|. .     S    * . |
|          . +    |
|           o o   |
|            o    |
|                 |
+----[SHA256]-----+";
        assert_eq!(randomart_for(&k), expected);
    }

    #[test]
    fn rsa_matches_ssh_keygen() {
        let k = key(concat!(
            "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCWHeC4t/CzGMPaxyIX5+jhgAfinThdJur8/l7EPV9YaZt+rXHk9Xt",
            "HP0+nfLlb97q5hLOqZZ7VvoeaIlhMkS3DvGO66x+XTTl1i5BCcCxTqjG1hP/jmLpRXnnQxs2o/Cx8jP/herUXpibpk",
            "JfxpIL8OzpdAu3+jwtCtRoJW8au/Kk7sb/mPQ5/Mf6kanB4RWUMSYdvXD04Prpz4gu4NGTa3kKzT5pvvFl1OcuOhdG",
            "3Z/bf5NFUi1LmRS8vgr1Viz2B7krYSJoAXhjENYOHblAGweQCsXYvVYrSLb8W6WgAPy3k9rhyZwVcqWb0CwdRd9Gki",
            "LvkHARuezokwBfMi0qjR7rMtCUL38Be+64wIgdkE9P7CmtkNNxlIocvsgOTCctvXzKJGwcF3ga1KD0lP2Wb32yCHe+",
            "fCJsnkl4quWazK4VYfutR0Ab37Dj8P3Ol9n3rAArKnHhmvgIebiKBe2YcWKF44tnvjmJFS5/qiQ1lb6LPKro81H3lc",
            "dxmmmYfVe8= rsa"
        ));
        assert_eq!(
            sha256_fingerprint(k.key_data()),
            "SHA256:U30E+8kygEbWyIY1GOTwegYCruSm2oJ4fP28ugk8ZhE"
        );
        let expected = "\
+---[RSA 3072]----+
|.   ..o*=o  ...  |
|..   +oo+o.. o   |
| o. E o.o o o .  |
|+  . + . . . + . |
|.o  o o S   o +  |
|o  . +   .   o   |
|+.  *.           |
|=.oo.o.o         |
|.o..  +++.       |
+----[SHA256]-----+";
        assert_eq!(randomart_for(&k), expected);
    }

    #[test]
    fn ecdsa_matches_ssh_keygen() {
        let k = key(concat!(
            "ecdsa-sha2-nistp384 AAAAE2VjZHNhLXNoYTItbmlzdHAzODQAAAAIbmlzdHAzODQAAABhBON6WkFB9YC8FqAqLe",
            "/0XqIug3DvIsD8C3QzzVlwNRS1vMZORZs7tmPPZBSckJrryYIXQ/eNdiAf6938L1LTdrjoCRgLpgdNMCOk9dKpWRuV",
            "b9NSVJ401uhs7wKzkRc/iw== e"
        ));
        let expected = "\
+---[ECDSA 384]---+
|                 |
|           . + . |
|           .=.=. |
|          o...o=+|
|       .S. .= ++=|
|    . =.oo + * =*|
|     O.+= . + +.O|
|    .E+.+o   o * |
|      .. o    o  |
+----[SHA256]-----+";
        assert_eq!(randomart_for(&k), expected);
        assert_eq!(key_type_and_bits(k.key_data()), ("ECDSA", 384));
    }
}
