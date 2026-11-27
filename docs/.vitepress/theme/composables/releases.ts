// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Latest release lookup through the public GitHub REST API, straight from the visitor's
 * browser (no proxy, no token). Assets are classified by the file names that
 * `.github/workflows/release.yml` publishes (tauri-action for desktop, renamed APK/IPA for mobile,
 * plus `SHA256SUMS.txt`, its Sigstore bundle `SHA256SUMS.txt.sigstore.json` and
 * `sbom.spdx.json`). Updater files (`*.sig`, `*.app.tar.gz`, `latest.json`) are ignored.
 */
import { onMounted, ref, type Ref } from 'vue';

import { LATEST_RELEASE_API } from '../../site';
import type { Arch, OsId } from './platform';

export interface GithubAsset {
  name: string;
  size: number;
  browser_download_url: string;
  content_type?: string;
}

interface GithubRelease {
  tag_name: string;
  name: string | null;
  html_url: string;
  published_at: string | null;
  prerelease: boolean;
  assets: GithubAsset[];
}

export interface AssetKind {
  id: string;
  os: OsId;
  arch: Arch | 'universal';
  label: string;
  detail: string;
  pattern: RegExp;
}

/** Installable files, most recommended first within each platform. */
export const ASSET_KINDS: AssetKind[] = [
  {
    id: 'win-x64-setup',
    os: 'windows',
    arch: 'x64',
    label: 'Installer',
    detail: '.exe · x64',
    pattern: /_x64-setup\.exe$/i,
  },
  {
    id: 'win-x64-msi',
    os: 'windows',
    arch: 'x64',
    label: 'MSI package',
    detail: '.msi · x64',
    pattern: /_x64_[a-z]{2}-[a-z]{2}\.msi$/i,
  },
  {
    id: 'win-arm64-setup',
    os: 'windows',
    arch: 'arm64',
    label: 'Installer',
    detail: '.exe · ARM64',
    pattern: /_arm64-setup\.exe$/i,
  },
  {
    id: 'win-arm64-msi',
    os: 'windows',
    arch: 'arm64',
    label: 'MSI package',
    detail: '.msi · ARM64',
    pattern: /_arm64_[a-z]{2}-[a-z]{2}\.msi$/i,
  },
  {
    id: 'mac-universal',
    os: 'macos',
    arch: 'universal',
    label: 'Disk image',
    detail: '.dmg · Apple silicon & Intel',
    pattern: /_universal\.dmg$/i,
  },
  {
    id: 'mac-arm64',
    os: 'macos',
    arch: 'arm64',
    label: 'Disk image',
    detail: '.dmg · Apple silicon',
    pattern: /_aarch64\.dmg$/i,
  },
  {
    id: 'mac-x64',
    os: 'macos',
    arch: 'x64',
    label: 'Disk image',
    detail: '.dmg · Intel',
    pattern: /_x64\.dmg$/i,
  },
  {
    id: 'linux-appimage-x64',
    os: 'linux',
    arch: 'x64',
    label: 'AppImage',
    detail: 'Any distribution · x86_64',
    pattern: /_amd64\.AppImage$/i,
  },
  {
    id: 'linux-deb-x64',
    os: 'linux',
    arch: 'x64',
    label: 'Debian / Ubuntu',
    detail: '.deb · amd64',
    pattern: /_amd64\.deb$/i,
  },
  {
    id: 'linux-rpm-x64',
    os: 'linux',
    arch: 'x64',
    label: 'Fedora / RHEL / openSUSE',
    detail: '.rpm · x86_64',
    pattern: /\.x86_64\.rpm$/i,
  },
  {
    id: 'linux-appimage-arm64',
    os: 'linux',
    arch: 'arm64',
    label: 'AppImage',
    detail: 'Any distribution · aarch64',
    pattern: /_(aarch64|arm64)\.AppImage$/i,
  },
  {
    id: 'linux-deb-arm64',
    os: 'linux',
    arch: 'arm64',
    label: 'Debian / Ubuntu',
    detail: '.deb · arm64',
    pattern: /_(arm64|aarch64)\.deb$/i,
  },
  {
    id: 'linux-rpm-arm64',
    os: 'linux',
    arch: 'arm64',
    label: 'Fedora / RHEL / openSUSE',
    detail: '.rpm · aarch64',
    pattern: /\.aarch64\.rpm$/i,
  },
  {
    id: 'android-apk',
    os: 'android',
    arch: 'universal',
    label: 'APK',
    detail: 'Universal · Android 8+',
    pattern: /_android-universal\.apk$/i,
  },
  {
    id: 'ios-ipa',
    os: 'ios',
    arch: 'universal',
    label: 'IPA',
    detail: 'iPhone & iPad · sideload',
    pattern: /_ios\.ipa$/i,
  },
];

export interface MatchedAsset {
  kind: AssetKind;
  asset: GithubAsset;
}

export interface ReleaseInfo {
  version: string;
  tag: string;
  name: string;
  url: string;
  publishedAt: string | null;
  prerelease: boolean;
  installers: MatchedAsset[];
  checksums: GithubAsset | null;
  /** Sigstore bundle signing `SHA256SUMS.txt`. */
  checksumsBundle: GithubAsset | null;
  sbom: GithubAsset | null;
  assetCount: number;
}

export type ReleaseState =
  | { status: 'loading' }
  | { status: 'ready'; release: ReleaseInfo }
  | { status: 'none' }
  | { status: 'unavailable'; reason: 'rate-limited' | 'offline' | 'error' };

/** Signatures and other companions that must never be offered as an installer. */
const NOT_INSTALLER = /\.(sig|asc|pem|crt|bundle|sigstore\.json|intoto\.jsonl|tar\.gz)$/i;

export function classifyRelease(release: GithubRelease): ReleaseInfo {
  const installers: MatchedAsset[] = [];
  for (const kind of ASSET_KINDS) {
    const matches = release.assets.filter(
      (a) => !NOT_INSTALLER.test(a.name) && kind.pattern.test(a.name),
    );
    // WiX builds one MSI per language (en-US, de-DE); offer the English one by default.
    const asset = matches.find((a) => /_en-US\.msi$/i.test(a.name)) ?? matches[0];
    if (asset) installers.push({ kind, asset });
  }
  return {
    version: release.tag_name.replace(/^v/i, ''),
    tag: release.tag_name,
    name: release.name ?? release.tag_name,
    url: release.html_url,
    publishedAt: release.published_at,
    prerelease: release.prerelease,
    installers,
    checksums: release.assets.find((a) => /^SHA256SUMS(\.txt)?$/i.test(a.name)) ?? null,
    checksumsBundle:
      release.assets.find((a) => /^SHA256SUMS(\.txt)?\.sigstore\.json$/i.test(a.name)) ?? null,
    sbom: release.assets.find((a) => /\.spdx\.json$/i.test(a.name)) ?? null,
    assetCount: release.assets.length,
  };
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '';
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${value.toFixed(value >= 100 || unit === 0 ? 0 : 1)} ${units[unit] ?? 'B'}`;
}

const CACHE_KEY = 'vaultport:latest-release:v1';
const CACHE_TTL_MS = 15 * 60 * 1000;

function readCache(): GithubRelease | null | undefined {
  try {
    const raw = sessionStorage.getItem(CACHE_KEY);
    if (!raw) return undefined;
    const cached = JSON.parse(raw) as { at: number; release: GithubRelease | null };
    if (Date.now() - cached.at > CACHE_TTL_MS) return undefined;
    return cached.release;
  } catch {
    return undefined;
  }
}

function writeCache(release: GithubRelease | null): void {
  try {
    sessionStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), release }));
  } catch {
    // Storage can be unavailable (private mode, blocked site data); caching is optional.
  }
}

export async function fetchLatestRelease(signal?: AbortSignal): Promise<ReleaseState> {
  const cached = readCache();
  if (cached !== undefined) {
    return cached ? { status: 'ready', release: classifyRelease(cached) } : { status: 'none' };
  }
  let response: Response;
  try {
    response = await fetch(LATEST_RELEASE_API, {
      headers: { Accept: 'application/vnd.github+json', 'X-GitHub-Api-Version': '2022-11-28' },
      signal,
    });
  } catch {
    return { status: 'unavailable', reason: 'offline' };
  }
  if (response.status === 404) {
    writeCache(null);
    return { status: 'none' };
  }
  if (response.status === 403 || response.status === 429) {
    return { status: 'unavailable', reason: 'rate-limited' };
  }
  if (!response.ok) return { status: 'unavailable', reason: 'error' };
  try {
    const release = (await response.json()) as GithubRelease;
    if (!Array.isArray(release.assets)) return { status: 'unavailable', reason: 'error' };
    writeCache(release);
    return { status: 'ready', release: classifyRelease(release) };
  } catch {
    return { status: 'unavailable', reason: 'error' };
  }
}

/** Fetches the latest release after mount; the server-rendered page shows the loading state. */
export function useLatestRelease(): Ref<ReleaseState> {
  const state = ref<ReleaseState>({ status: 'loading' });
  onMounted(() => {
    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), 10_000);
    void fetchLatestRelease(controller.signal).then((result) => {
      window.clearTimeout(timeout);
      state.value = result;
    });
  });
  return state;
}

/** Picks the best installer for a platform (exact arch, then universal, then anything). */
export function pickInstaller(
  installers: MatchedAsset[],
  os: OsId,
  arch: Arch,
): MatchedAsset | null {
  const forOs = installers.filter((m) => m.kind.os === os);
  return (
    forOs.find((m) => m.kind.arch === 'universal') ??
    forOs.find((m) => m.kind.arch === arch) ??
    forOs.find((m) => m.kind.arch === 'x64') ??
    forOs[0] ??
    null
  );
}
