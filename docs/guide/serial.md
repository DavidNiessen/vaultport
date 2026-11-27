---
title: Serial consoles
description: Open serial consoles over USB adapters and console cables with configurable baud rate, data bits, parity, stop bits and flow control.
---

# Serial consoles

Connect to network gear, single-board computers, microcontrollers and servers over a **serial console** — a USB-to-serial adapter, a USB console port or a built-in COM port.

## Opening a console

1. In the **Hosts** view, click **Serial** in the toolbar.
2. **Choose a port** from the list — USB adapters show their vendor and product — or **enter a device path** such as `/dev/ttyUSB0`, `/dev/tty.usbserial-1410` or `COM3`.
3. Set the line parameters and connect.

| Setting      | Typical value  | Options                                       |
| ------------ | -------------- | --------------------------------------------- |
| Baud rate    | 9600 or 115200 | 300 to 921600 (115200 preselected)            |
| Data bits    | 8              | 5, 6, 7, 8                                    |
| Parity       | None           | None, Odd, Even                               |
| Stop bits    | 1              | 1, 2                                          |
| Flow control | None           | None, hardware (RTS/CTS), software (XON/XOFF) |

Most Cisco, Juniper and Aruba consoles use **9600 8N1** without flow control; many Linux boards and Raspberry Pis use **115200 8N1**.

## Platforms and permissions

Serial consoles are available in the **desktop** apps (Windows, macOS, Linux).

- **Linux:** add yourself to the `dialout` group (Debian, Ubuntu, Fedora) or `uucp` (Arch), then log in again.
- **macOS:** most adapters work out of the box; some need the vendor's driver (for example for older Prolific or CH340 chips).
- **Windows:** the adapter appears as `COMn` in Device Manager.

## Logging

Serial sessions are recorded to [encrypted session logs](/guide/session-logs), which is useful for keeping a record of console work on network equipment.
