// SPDX-License-Identifier: GPL-3.0-or-later
//! Serial port discovery.

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// A serial port found on the system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortInfo {
    /// Path to open (`/dev/ttyUSB0`, `/dev/cu.usbserial-1410`, `COM3`).
    pub path: String,
    /// How the port is attached.
    pub kind: PortKind,
    /// Human readable description, when known.
    pub description: Option<String>,
}

/// How a serial port is attached.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PortKind {
    /// USB serial adapter or USB CDC-ACM device.
    #[serde(rename_all = "camelCase")]
    Usb {
        /// USB vendor id.
        vid: u16,
        /// USB product id.
        pid: u16,
        /// Serial number string.
        serial_number: Option<String>,
        /// Manufacturer string.
        manufacturer: Option<String>,
        /// Product string.
        product: Option<String>,
    },
    /// Bluetooth serial port profile.
    Bluetooth,
    /// Built-in (PCI / platform) UART.
    Pci,
    /// Unknown attachment (including pseudo terminals).
    Unknown,
}

/// Lists the serial ports of this machine, sorted by path.
///
/// On macOS only the call-out devices (`/dev/cu.*`) are returned: the matching
/// `/dev/tty.*` dial-in devices block on open until carrier detect is raised.
/// Without the `native` feature (and on Android/iOS) the list is empty; the app's
/// transport plugin enumerates devices there.
#[cfg_attr(
    not(all(feature = "native", not(any(target_os = "android", target_os = "ios")))),
    allow(clippy::unnecessary_wraps)
)]
pub fn list_ports() -> Result<Vec<PortInfo>> {
    #[cfg(all(feature = "native", not(any(target_os = "android", target_os = "ios"))))]
    {
        let ports = serialport::available_ports()
            .map_err(|e| crate::error::Error::Port(e.description))?
            .into_iter()
            .map(from_serialport)
            .collect();
        let ports = if cfg!(target_os = "macos") {
            prefer_callout_devices(ports)
        } else {
            ports
        };
        Ok(sorted(ports))
    }
    #[cfg(not(all(feature = "native", not(any(target_os = "android", target_os = "ios")))))]
    {
        Ok(Vec::new())
    }
}

#[cfg(all(feature = "native", not(any(target_os = "android", target_os = "ios"))))]
fn from_serialport(info: serialport::SerialPortInfo) -> PortInfo {
    let (kind, description) = match info.port_type {
        serialport::SerialPortType::UsbPort(usb) => {
            let description = match (&usb.manufacturer, &usb.product) {
                (Some(m), Some(p)) if !p.contains(m.as_str()) => Some(format!("{m} {p}")),
                (_, Some(p)) => Some(p.clone()),
                (Some(m), None) => Some(m.clone()),
                (None, None) => None,
            };
            (
                PortKind::Usb {
                    vid: usb.vid,
                    pid: usb.pid,
                    serial_number: usb.serial_number,
                    manufacturer: usb.manufacturer,
                    product: usb.product,
                },
                description,
            )
        }
        serialport::SerialPortType::BluetoothPort => {
            (PortKind::Bluetooth, Some("Bluetooth".to_owned()))
        }
        serialport::SerialPortType::PciPort => (PortKind::Pci, None),
        serialport::SerialPortType::Unknown => (PortKind::Unknown, None),
    };
    PortInfo {
        path: info.port_name,
        kind,
        description,
    }
}

#[cfg_attr(
    not(all(feature = "native", not(any(target_os = "android", target_os = "ios")))),
    allow(dead_code)
)]
fn sorted(mut ports: Vec<PortInfo>) -> Vec<PortInfo> {
    ports.sort_by(|a, b| a.path.cmp(&b.path));
    ports.dedup_by(|a, b| a.path == b.path);
    ports
}

/// Drops `/dev/tty.X` entries whose call-out twin `/dev/cu.X` is present.
#[cfg_attr(
    not(all(
        target_os = "macos",
        feature = "native",
        not(any(target_os = "android", target_os = "ios"))
    )),
    allow(dead_code)
)]
fn prefer_callout_devices(ports: Vec<PortInfo>) -> Vec<PortInfo> {
    let callouts: std::collections::HashSet<String> = ports
        .iter()
        .filter_map(|p| p.path.strip_prefix("/dev/cu.").map(str::to_owned))
        .collect();
    ports
        .into_iter()
        .filter(|p| {
            p.path
                .strip_prefix("/dev/tty.")
                .is_none_or(|name| !callouts.contains(name))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn port(path: &str) -> PortInfo {
        PortInfo {
            path: path.to_owned(),
            kind: PortKind::Unknown,
            description: None,
        }
    }

    #[test]
    fn callout_devices_win_on_macos() {
        let ports = vec![
            port("/dev/tty.usbserial-1"),
            port("/dev/cu.usbserial-1"),
            port("/dev/tty.Bluetooth-Incoming-Port"),
        ];
        let paths: Vec<_> = prefer_callout_devices(ports)
            .into_iter()
            .map(|p| p.path)
            .collect();
        assert_eq!(
            paths,
            ["/dev/cu.usbserial-1", "/dev/tty.Bluetooth-Incoming-Port"]
        );
    }

    #[test]
    fn sorting_and_dedup() {
        let paths: Vec<_> = sorted(vec![port("COM3"), port("COM1"), port("COM3")])
            .into_iter()
            .map(|p| p.path)
            .collect();
        assert_eq!(paths, ["COM1", "COM3"]);
    }

    #[test]
    fn port_info_serializes_camel_case_tagged() {
        let info = PortInfo {
            path: "/dev/ttyUSB0".into(),
            kind: PortKind::Usb {
                vid: 0x0403,
                pid: 0x6001,
                serial_number: Some("A1".into()),
                manufacturer: Some("FTDI".into()),
                product: None,
            },
            description: Some("FTDI".into()),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "path": "/dev/ttyUSB0",
                "kind": {
                    "type": "usb",
                    "vid": 1027,
                    "pid": 24577,
                    "serialNumber": "A1",
                    "manufacturer": "FTDI",
                    "product": null
                },
                "description": "FTDI"
            })
        );
        assert_eq!(serde_json::from_value::<PortInfo>(json).unwrap(), info);
        assert_eq!(
            serde_json::to_value(PortKind::Bluetooth).unwrap(),
            serde_json::json!({"type": "bluetooth"})
        );
    }

    #[test]
    fn listing_ports_does_not_fail() {
        // The CI machine may have no serial ports; the call itself must succeed.
        let ports = list_ports().unwrap();
        assert!(ports.windows(2).all(|w| w[0].path < w[1].path));
    }
}
