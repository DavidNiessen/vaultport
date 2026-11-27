# Governance

Vaultport is an independent open-source project licensed under GPL-3.0-or-later.

## Roles

- **Maintainers** review and merge pull requests, triage issues, cut releases and handle
  security reports. They are listed in [`.github/CODEOWNERS`](.github/CODEOWNERS).
- **Contributors** are everyone who opens issues, discussions or pull requests.

## Decision making

Day-to-day decisions are made through pull-request review. Larger changes (new protocols,
storage format changes, cryptography, new network-facing features) start as an
**Architecture Decision Record** in [`docs/adr/`](docs/adr/) and need approval from at least
one maintainer. Changes that touch cryptography or the vault format need two approvals once
the project has more than one maintainer.

## Non-negotiable principles

1. **Serverless.** No feature may depend on infrastructure operated by the project.
2. **No telemetry.** The app only talks to hosts the user configured, plus clearly disclosed,
   optional endpoints (update checks, model download).
3. **Secure by default.** Insecure options exist only as explicit, per-host opt-ins.

## Becoming a maintainer

Sustained, high-quality contributions and reviews over several months are the path to
maintainership. Existing maintainers nominate and approve new ones.
