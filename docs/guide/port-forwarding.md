---
title: Port forwarding
description: Saved local, remote and dynamic (SOCKS5) SSH tunnels that start on demand or automatically when the vault unlocks.
---

# Port forwarding

**Port Forwarding** in the vault holds saved tunnels. Each rule goes through one SSH host — and that host's jump hosts and proxy, if any.

## Rule types

| Type        | OpenSSH equivalent | Use it to…                                                                       |
| ----------- | ------------------ | -------------------------------------------------------------------------------- |
| **Local**   | `-L`               | reach a service that is only reachable from the server (a database, an admin UI) |
| **Remote**  | `-R`               | make a service on your device reachable from the server                          |
| **Dynamic** | `-D`               | turn your device into a SOCKS proxy (SOCKS5 and SOCKS4a) into the remote network |

Example — open a database that only listens inside a private network:

| Field               | Value              |
| ------------------- | ------------------ |
| Forwarding type     | Local              |
| SSH host            | `bastion-prod`     |
| Local address       | `127.0.0.1:5432`   |
| Destination address | `db.internal:5432` |

Then point your database client at `127.0.0.1:5432`.

## Starting and stopping

- **Double-click** a rule (or use **Start** / **Stop**). The rule shows whether it is active and how many connections it currently carries.
- Enable **Start automatically when the vault unlocks** for tunnels you always want.
- Locking the vault stops all tunnels.

Tunnels and SFTP panes share a pool of SSH connections, so several tunnels or file panes for the same host reuse one login.

## Binding to other interfaces

By default local and dynamic forwards listen on `127.0.0.1`. You can bind to another address such as `0.0.0.0`, and Vaultport will warn you: **the tunnel is then reachable from other machines on your network** — for a dynamic forward, that is an open proxy into the remote network. For remote forwards, binding to non-loopback addresses on the server also requires `GatewayPorts` in the server's `sshd_config`.

## Importing forwards

`LocalForward`, `RemoteForward` and `DynamicForward` lines from `~/.ssh/config` and PuTTY `PortForwardings` become rules on import — see [Importing](/guide/importing).
