---
title: Groups & inheritance
description: Nestable groups whose settings are inherited per parameter by every host below them, with overrides at any level.
---

# Groups & inheritance

Vaultport organizes hosts in **groups**. Groups can be nested (for example `Production › EU › Web`), and — unlike plain folders — they carry **settings** that apply to every host and subgroup below them.

## How inheritance works

Every host and every group has a _sparse_ configuration: it only stores the settings you actually changed at that level. When you connect, Vaultport resolves each setting **individually** by walking up from the host:

1. If the host sets the value, use it.
2. Otherwise use the value from the closest parent group that sets it.
3. Otherwise use the built-in default.

```text
Production            username = deploy, jump host = bastion-prod, keepalive = 30 s
└── EU                port = 2222
    └── Web           theme = Ember (production)
        ├── web-01    (no overrides)  →  deploy@web-01:2222 via bastion-prod, Ember theme
        └── web-02    username = admin →  admin@web-02:2222 via bastion-prod, Ember theme
```

Because inheritance is per parameter, `web-02` can override only the username and still inherit the port, the jump host and the theme.

The host and group editors show which values are **Inherited** and from where, and a group's editor tells you how many hosts its settings currently apply to.

## What can be inherited

Practically everything that describes _how_ to connect:

- **SSH**: port, credentials (an [identity](/concepts/identities-and-keychain) or inline username, password and keys), jump hosts, proxy, agent forwarding (and confirmation), environment variables, startup snippet, keepalive interval, compression, legacy algorithms, post-quantum key exchange, host-key policy and connect timeout.
- **Mosh**: enabled and server command.
- **Telnet**: enabled, port, username and password.
- **Terminal**: theme, font family and size, backspace key (DEL or Ctrl+H), terminal type, encoding, scrollback, cursor blink, bright bold.
- **SFTP**: start path, show hidden files, follow symlinks.

The address, label, tags, notes and favorite flag belong to each host.

## Defaults

| Setting             | Default                                  |
| ------------------- | ---------------------------------------- |
| SSH port            | 22                                       |
| Telnet port         | 23                                       |
| Keepalive           | 30 seconds                               |
| Connect timeout     | 15 seconds                               |
| Post-quantum KEX    | on                                       |
| Host-key policy     | ask on first use, reject on change       |
| Terminal type       | `xterm-256color`                         |
| Theme / font        | Midnight · JetBrains Mono 14             |
| Mosh server command | `mosh-server new -s -l LANG=en_US.UTF-8` |

## Tips

- Put shared credentials on a group via an identity, so rotating a password or key is a single change.
- Give production groups a distinctive terminal theme such as **Ember (production)** so you always know where you are typing.
- Cloud inventory integrations create and maintain their own group; you can still add settings to it, and hosts inherit them. See [Cloud inventory](/guide/cloud-inventory).
