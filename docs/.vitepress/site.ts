// SPDX-License-Identifier: GPL-3.0-or-later
/** Facts about the project shared by the site config, the theme and the llms.txt generator. */

export const REPO = 'DavidNiessen/vaultport';
export const REPO_URL = `https://github.com/${REPO}`;
export const RELEASES_URL = `${REPO_URL}/releases`;
export const LATEST_RELEASE_API = `https://api.github.com/repos/${REPO}/releases/latest`;
export const SECURITY_URL = `${REPO_URL}/security/policy`;
export const ADVISORY_URL = `${REPO_URL}/security/advisories/new`;
export const LICENSE_URL = `${REPO_URL}/blob/main/LICENSE`;

export const NAME = 'Vaultport';
export const TAGLINE = 'Secure, serverless SSH';
export const DESCRIPTION =
  'A secure, serverless, open-source SSH, SFTP, Mosh, Telnet and serial client for Windows, macOS, Linux, iOS and Android. No account, no telemetry, an end-to-end encrypted vault and sync only through storage you own.';
