---
title: Importing
description: Import hosts, groups, keys, port forwards and known hosts from OpenSSH, PuTTY, MobaXterm, SecureCRT and CSV files.
---

# Importing

**Hosts → Import → File** brings your existing setup into the vault. Choose a file and ssh-client **detects the format** (UTF-8 or UTF-16 with BOM), shows a summary — hosts, groups, keys and known hosts imported, plus any warnings — and you are done. Imports add to your vault; within one import, hosts are de-duplicated by address, port and username.

## Supported formats

| Source                | What to choose                                         | What is imported                                                                                      |
| --------------------- | ------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| OpenSSH `ssh_config`  | `~/.ssh/config` (offered automatically when it exists) | Hosts, `Include`d files, `Host *` defaults as a group, `ProxyJump`, forwards, `IdentityFile` keys     |
| OpenSSH `known_hosts` | `~/.ssh/known_hosts`                                   | Trusted host keys, including hashed entries, `@cert-authority` and `@revoked`                         |
| PuTTY                 | A registry export (see below)                          | SSH and Telnet sessions, tunnels, `.ppk` keys, SOCKS5/HTTP proxies, SSH proxy as jump host, host keys |
| MobaXterm             | `.mobaconf` or `.mxtsession`                           | SSH and Telnet bookmarks, folders as nested groups, private key paths                                 |
| SecureCRT             | XML export (_File → Export Settings_)                  | SSH2 and Telnet sessions, folders as nested groups, identity files                                    |
| CSV                   | A spreadsheet export                                   | Hosts with label, address, port, user, password, group path, tags and protocol                        |

### OpenSSH config

Each concrete `Host` alias becomes a host, with its settings computed the way `ssh -G` does (first value wins, `IdentityFile` and forwards accumulate). Options that apply to _every_ host (`Host *`, `Match all`, top-level options) go into one shared group, so they are [inherited](/concepts/groups-and-inheritance) exactly like before. Other wildcard blocks such as `Host *.prod.example.com` are applied to the matching hosts.

Not supported, with a warning: `Match` blocks other than `Match all`, and `ProxyCommand`.

Private keys referenced by `IdentityFile` are read and stored encrypted in the vault; encrypted keys stay encrypted and ask for their passphrase on first use.

### PuTTY

Export your sessions on Windows with:

```powershell
reg export HKCU\Software\SimonTatham\PuTTY putty.reg
```

`PortForwardings` (for example `L8080=localhost:80,R2222=localhost:22,D1080`) become [port-forwarding rules](/guide/port-forwarding), PuTTY's cached `SshHostKeys` become known hosts, and a proxy password becomes an encrypted vault secret.

### MobaXterm and SecureCRT

Folders become nested groups. Passwords stored by these programs are encrypted with their own master passwords and **cannot** be imported — the summary tells you how many were skipped. MobaXterm's SSH gateway (jump host) settings are not imported; add jump hosts afterwards.

### CSV

The first row is a header; columns are matched by name in any order and only `address` is required:

```text
label,address,port,username,password,group,tags,protocol
web-01,10.0.1.11,22,deploy,,Prod/EU,web;nginx,ssh
switch-core,10.0.0.2,23,admin,,Network,cisco,telnet
```

- `group` is a path nested with `/`, `tags` are separated by `;`,
- `protocol` is `ssh` (default), `telnet` or `mosh`,
- passwords are stored as encrypted vault secrets.

## Safety

Import files are treated as untrusted input: files over 16 MiB are rejected, nesting and `Include` depth are bounded, malformed entries are skipped with a warning (which never contains secret values), and every imported host, group and forward is validated before it is stored.

## From the cloud

To keep a group in sync with your AWS, DigitalOcean or Azure account instead of importing once, see [Cloud inventory](/guide/cloud-inventory).
