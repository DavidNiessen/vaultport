// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial line settings.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Lowest accepted baud rate.
pub const MIN_BAUD_RATE: u32 = 50;
/// Highest accepted baud rate (fast USB UARTs such as FT232H reach 12 Mbaud).
pub const MAX_BAUD_RATE: u32 = 16_000_000;

/// Baud rates offered by the UI. Any rate in `MIN_BAUD_RATE..=MAX_BAUD_RATE` is accepted;
/// whether the hardware supports it is up to the driver.
pub const COMMON_BAUD_RATES: &[u32] = &[
    300, 1_200, 2_400, 4_800, 9_600, 19_200, 38_400, 57_600, 74_880, 115_200, 230_400, 250_000,
    460_800, 500_000, 921_600, 1_000_000, 1_500_000, 2_000_000, 3_000_000,
];

/// Number of data bits per character. Serialized as the number (`5`..=`8`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum DataBits {
    /// 5 bits.
    Five,
    /// 6 bits.
    Six,
    /// 7 bits.
    Seven,
    /// 8 bits.
    #[default]
    Eight,
}

impl TryFrom<u8> for DataBits {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            5 => Ok(Self::Five),
            6 => Ok(Self::Six),
            7 => Ok(Self::Seven),
            8 => Ok(Self::Eight),
            _ => Err(Error::InvalidConfig("data bits must be 5, 6, 7 or 8")),
        }
    }
}

impl From<DataBits> for u8 {
    fn from(value: DataBits) -> Self {
        match value {
            DataBits::Five => 5,
            DataBits::Six => 6,
            DataBits::Seven => 7,
            DataBits::Eight => 8,
        }
    }
}

/// Number of stop bits. Serialized as the number (`1` or `2`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub enum StopBits {
    /// One stop bit.
    #[default]
    One,
    /// Two stop bits.
    Two,
}

impl TryFrom<u8> for StopBits {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            _ => Err(Error::InvalidConfig("stop bits must be 1 or 2")),
        }
    }
}

impl From<StopBits> for u8 {
    fn from(value: StopBits) -> Self {
        match value {
            StopBits::One => 1,
            StopBits::Two => 2,
        }
    }
}

/// Parity checking.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Parity {
    /// No parity bit.
    #[default]
    None,
    /// Odd parity.
    Odd,
    /// Even parity.
    Even,
}

/// Flow control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FlowControl {
    /// No flow control.
    #[default]
    None,
    /// XON/XOFF in-band flow control.
    Software,
    /// RTS/CTS hardware flow control.
    Hardware,
}

/// Serial line configuration. Defaults to 115200 8N1 without flow control; missing fields
/// in serialized form take their defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct SerialConfig {
    /// Line speed in bits per second.
    pub baud_rate: u32,
    /// Data bits per character.
    pub data_bits: DataBits,
    /// Stop bits.
    pub stop_bits: StopBits,
    /// Parity.
    pub parity: Parity,
    /// Flow control.
    pub flow_control: FlowControl,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self::new(115_200)
    }
}

impl SerialConfig {
    /// 8N1 without flow control at `baud_rate`.
    pub const fn new(baud_rate: u32) -> Self {
        Self {
            baud_rate,
            data_bits: DataBits::Eight,
            stop_bits: StopBits::One,
            parity: Parity::None,
            flow_control: FlowControl::None,
        }
    }

    /// Checks that the settings are in range. Enum fields are valid by construction.
    pub fn validate(self) -> Result<()> {
        if !(MIN_BAUD_RATE..=MAX_BAUD_RATE).contains(&self.baud_rate) {
            return Err(Error::InvalidConfig("baud rate out of range"));
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_115200_8n1() {
        let config = SerialConfig::default();
        assert_eq!(config.baud_rate, 115_200);
        assert_eq!(u8::from(config.data_bits), 8);
        assert_eq!(u8::from(config.stop_bits), 1);
        assert_eq!(config.parity, Parity::None);
        assert_eq!(config.flow_control, FlowControl::None);
        config.validate().unwrap();
    }

    #[test]
    fn validates_baud_rate() {
        for &rate in COMMON_BAUD_RATES {
            SerialConfig::new(rate).validate().unwrap();
        }
        assert!(SerialConfig::new(0).validate().is_err());
        assert!(SerialConfig::new(49).validate().is_err());
        assert!(SerialConfig::new(MAX_BAUD_RATE + 1).validate().is_err());
        SerialConfig::new(MIN_BAUD_RATE).validate().unwrap();
        SerialConfig::new(MAX_BAUD_RATE).validate().unwrap();
    }

    #[test]
    fn data_and_stop_bits_conversions() {
        for bits in 5..=8 {
            assert_eq!(u8::from(DataBits::try_from(bits).unwrap()), bits);
        }
        assert!(DataBits::try_from(4).is_err());
        assert!(DataBits::try_from(9).is_err());
        assert_eq!(u8::from(StopBits::try_from(2).unwrap()), 2);
        assert!(StopBits::try_from(0).is_err());
        assert!(StopBits::try_from(3).is_err());
    }

    #[test]
    fn serde_uses_camel_case_and_numbers() {
        let config = SerialConfig {
            baud_rate: 9_600,
            data_bits: DataBits::Seven,
            stop_bits: StopBits::Two,
            parity: Parity::Even,
            flow_control: FlowControl::Hardware,
        };
        let json = serde_json::to_value(config).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "baudRate": 9600,
                "dataBits": 7,
                "stopBits": 2,
                "parity": "even",
                "flowControl": "hardware"
            })
        );
        let back: SerialConfig = serde_json::from_value(json).unwrap();
        assert_eq!(back, config);

        let partial: SerialConfig = serde_json::from_str(r#"{"baudRate": 57600}"#).unwrap();
        assert_eq!(partial, SerialConfig::new(57_600));
    }

    #[test]
    fn serde_rejects_invalid_values() {
        for bad in [
            r#"{"dataBits": 9}"#,
            r#"{"dataBits": 4}"#,
            r#"{"stopBits": 3}"#,
            r#"{"stopBits": 1.5}"#,
            r#"{"parity": "mark"}"#,
            r#"{"flowControl": "rtscts"}"#,
            r#"{"baudRate": -1}"#,
            r#"{"baud": 9600}"#,
        ] {
            assert!(serde_json::from_str::<SerialConfig>(bad).is_err(), "{bad}");
        }
    }
}
