---
title: Mosh
description: Use the built-in Mosh client for sessions that survive network changes, roaming and sleep — bootstrapped over SSH, no mosh-client binary required.
---

# Mosh

[Mosh](https://mosh.org/) (mobile shell) keeps a session alive when your network changes — switching from Wi-Fi to mobile data, closing the laptop lid, roaming between access points. Vaultport contains a **native Mosh client** written in Rust, wire-compatible with `mosh-server` 1.3 and 1.4. You do not need to install `mosh-client` locally.

## Requirements

- `mosh-server` installed on the host (`apt install mosh`, `dnf install mosh`, `brew install mosh` …).
- UDP traffic to the server allowed — by default `mosh-server` picks a port between 60000 and 61000.
- Working SSH access: Mosh starts through SSH.

## Enabling Mosh

In the host (or group) editor, enable **Use Mosh**. Optionally change the **server command**; the default is:

```bash
mosh-server new -s -l LANG=en_US.UTF-8
```

## How a Mosh session starts

1. Vaultport connects over SSH as usual — with your keys, jump hosts, proxy and host-key checks.
2. It runs the server command, which prints `MOSH CONNECT <port> <key>` and detaches.
3. The SSH connection is no longer needed; the session continues over **UDP**, encrypted and authenticated with **AES-128-OCB3** using the session key from step 2.

## Roaming and resilience

- The server always replies to the address of the newest authentic packet, so Vaultport can switch networks at any time.
- Like the reference client, Vaultport hops to a new local port automatically after 10 seconds without a round trip.
- Mosh synchronizes the **screen state** rather than a byte stream: after a long disconnect you see the current screen immediately, without replaying everything that happened.
- Packets that fail authentication are dropped silently; replayed and wrong-direction packets are rejected.

## Limitations

Mosh carries an interactive terminal only. Port forwarding, agent forwarding and SFTP need an SSH connection — use a regular SSH tab or the SFTP tab for those. Scrollback is what Vaultport's terminal has received; Mosh itself does not resend history.
