---
title: Reporting a vulnerability
description: How to report security vulnerabilities in Vaultport privately, which versions are supported, and what to expect afterwards.
---

# Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report vulnerabilities privately through [GitHub Security Advisories](https://github.com/DavidNiessen/vaultport/security/advisories/new). Include:

- the affected version(s) and platform(s),
- a description of the issue and its impact,
- steps to reproduce or a proof of concept,
- any suggested mitigation.

## What happens next

| Step                         | Target              |
| ---------------------------- | ------------------- |
| Acknowledgement              | within **72 hours** |
| Initial assessment           | within **7 days**   |
| Fix for high/critical issues | within **30 days**  |

You will be kept informed and credited in the advisory unless you prefer to stay anonymous.

## Supported versions

| Version        | Supported                        |
| -------------- | -------------------------------- |
| Latest release | Yes                              |
| Previous minor | Security fixes only, for 90 days |
| Anything older | No                               |

## Scope

In scope:

- the desktop and mobile applications in the repository,
- the Rust crates under `crates/` and native plugins,
- the release pipeline (build provenance, signing, updater manifests).

Out of scope:

- vulnerabilities in SSH servers you connect to,
- issues that require a fully compromised operating system or a malicious, already-installed application with the same privileges as Vaultport,
- denial of service by the user against their own device.

The canonical version of this policy is [`SECURITY.md`](https://github.com/DavidNiessen/vaultport/blob/main/SECURITY.md) in the repository.
