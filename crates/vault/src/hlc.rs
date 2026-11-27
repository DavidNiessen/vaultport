// SPDX-License-Identifier: GPL-3.0-or-later
//! Hybrid logical clocks.
//!
//! A timestamp is `(wall_ms, counter, device)`; ordering is lexicographic, so concurrent writes
//! from different devices are totally ordered and every device converges on the same winner.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

/// Random identifier of a device participating in a vault.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeviceId(pub u64);

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// A hybrid logical clock timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u32,
    pub device: DeviceId,
}

impl Hlc {
    pub const ZERO: Self = Self {
        wall_ms: 0,
        counter: 0,
        device: DeviceId(0),
    };
}

impl Ord for Hlc {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.wall_ms, self.counter, self.device).cmp(&(other.wall_ms, other.counter, other.device))
    }
}

impl PartialOrd for Hlc {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Hlc {
    /// Fixed-width, lexicographically sortable encoding.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:013x}-{:08x}-{:016x}",
            self.wall_ms, self.counter, self.device.0
        )
    }
}

impl FromStr for Hlc {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split('-');
        let (Some(wall), Some(counter), Some(device), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(format!("invalid hlc {s:?}"));
        };
        let hex64 = |x: &str| u64::from_str_radix(x, 16).map_err(|e| e.to_string());
        Ok(Self {
            wall_ms: hex64(wall)?,
            counter: u32::from_str_radix(counter, 16).map_err(|e| e.to_string())?,
            device: DeviceId(hex64(device)?),
        })
    }
}

impl Serialize for Hlc {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Hlc {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Maximum tolerated clock skew of a remote timestamp (1 day). Timestamps further in the future
/// are clamped so a device with a broken clock cannot win every future conflict.
const MAX_SKEW_MS: u64 = 24 * 60 * 60 * 1000;

/// A clock issuing monotonically increasing [`Hlc`] timestamps for one device.
#[derive(Debug, Clone)]
pub struct Clock {
    device: DeviceId,
    last: Hlc,
    now: fn() -> u64,
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

impl Clock {
    #[must_use]
    pub fn new(device: DeviceId) -> Self {
        Self {
            device,
            last: Hlc::ZERO,
            now: system_now_ms,
        }
    }

    #[cfg(test)]
    fn with_time_source(device: DeviceId, now: fn() -> u64) -> Self {
        Self {
            device,
            last: Hlc::ZERO,
            now,
        }
    }

    #[must_use]
    pub const fn device(&self) -> DeviceId {
        self.device
    }

    /// Issues a new timestamp strictly greater than every timestamp seen so far.
    pub fn tick(&mut self) -> Hlc {
        let wall = (self.now)();
        let next = if wall > self.last.wall_ms {
            Hlc {
                wall_ms: wall,
                counter: 0,
                device: self.device,
            }
        } else {
            Hlc {
                wall_ms: self.last.wall_ms,
                counter: self.last.counter.saturating_add(1),
                device: self.device,
            }
        };
        self.last = next;
        next
    }

    /// Incorporates a timestamp received from another device.
    pub fn observe(&mut self, remote: Hlc) {
        let limit = (self.now)().saturating_add(MAX_SKEW_MS);
        let remote = if remote.wall_ms > limit {
            Hlc {
                wall_ms: limit,
                ..remote
            }
        } else {
            remote
        };
        if remote > self.last {
            self.last = Hlc {
                device: self.device,
                ..remote
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_even_when_wall_clock_stalls() {
        let mut c = Clock::with_time_source(DeviceId(1), || 1000);
        let a = c.tick();
        let b = c.tick();
        assert!(b > a);
        assert_eq!(b.counter, 1);
    }

    #[test]
    fn observe_moves_forward_and_clamps_skew() {
        let mut c = Clock::with_time_source(DeviceId(1), || 1000);
        c.observe(Hlc {
            wall_ms: 5000,
            counter: 3,
            device: DeviceId(2),
        });
        let t = c.tick();
        assert_eq!((t.wall_ms, t.counter), (5000, 4));

        let mut c = Clock::with_time_source(DeviceId(1), || 1000);
        c.observe(Hlc {
            wall_ms: u64::MAX / 2,
            counter: 0,
            device: DeviceId(2),
        });
        assert!(c.tick().wall_ms <= 1000 + MAX_SKEW_MS);
    }

    #[test]
    fn string_encoding_sorts_like_hlc() {
        let a = Hlc {
            wall_ms: 9,
            counter: 0xffff,
            device: DeviceId(5),
        };
        let b = Hlc {
            wall_ms: 10,
            counter: 0,
            device: DeviceId(1),
        };
        assert!(a < b);
        assert!(a.to_string() < b.to_string());
        assert_eq!(a.to_string().parse::<Hlc>(), Ok(a));
    }
}
