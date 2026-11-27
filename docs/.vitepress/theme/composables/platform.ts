// SPDX-License-Identifier: GPL-3.0-or-later
/** Client-side operating system and CPU architecture detection for download suggestions. */
import { onMounted, ref, type Ref } from 'vue';

export type OsId = 'windows' | 'macos' | 'linux' | 'android' | 'ios';
export type Arch = 'x64' | 'arm64' | 'unknown';

export interface DetectedPlatform {
  os: OsId | null;
  arch: Arch;
}

export const OS_LABEL: Record<OsId, string> = {
  windows: 'Windows',
  macos: 'macOS',
  linux: 'Linux',
  android: 'Android',
  ios: 'iOS & iPadOS',
};

interface UADataValues {
  platform?: string;
  architecture?: string;
  bitness?: string;
}

interface NavigatorUAData {
  platform: string;
  mobile: boolean;
  getHighEntropyValues(hints: string[]): Promise<UADataValues>;
}

function osFromPlatformName(name: string): OsId | null {
  const p = name.toLowerCase();
  if (p.includes('win')) return 'windows';
  if (p.includes('android')) return 'android';
  if (p.includes('ios') || p.includes('iphone') || p.includes('ipad')) return 'ios';
  if (p.includes('mac')) return 'macos';
  if (p.includes('linux') || p.includes('chrome os') || p.includes('cros')) return 'linux';
  return null;
}

/** Best-effort detection from the classic user agent string (synchronous). */
export function detectFromUserAgent(ua: string, maxTouchPoints = 0): DetectedPlatform {
  let os: OsId | null = null;
  if (/android/i.test(ua)) os = 'android';
  else if (/iphone|ipad|ipod/i.test(ua)) os = 'ios';
  else if (/macintosh|mac os x/i.test(ua)) os = maxTouchPoints > 1 ? 'ios' : 'macos';
  else if (/windows/i.test(ua)) os = 'windows';
  else if (/linux|x11|cros/i.test(ua)) os = 'linux';

  let arch: Arch = 'unknown';
  if (/aarch64|arm64|armv8/i.test(ua)) arch = 'arm64';
  else if (/x86_64|x64|win64|wow64|amd64/i.test(ua)) arch = 'x64';
  return { os, arch };
}

/** Uses User-Agent Client Hints where available (Chromium), falling back to the UA string. */
export async function detectPlatform(): Promise<DetectedPlatform> {
  const fallback = detectFromUserAgent(navigator.userAgent, navigator.maxTouchPoints);
  const uaData = (navigator as Navigator & { userAgentData?: NavigatorUAData }).userAgentData;
  if (!uaData) return fallback;
  try {
    const values = await uaData.getHighEntropyValues(['platform', 'architecture', 'bitness']);
    const os = osFromPlatformName(values.platform ?? uaData.platform) ?? fallback.os;
    let arch: Arch = fallback.arch;
    if (values.architecture === 'arm') arch = values.bitness === '32' ? 'unknown' : 'arm64';
    else if (values.architecture === 'x86') arch = values.bitness === '32' ? 'unknown' : 'x64';
    return { os, arch };
  } catch {
    return fallback;
  }
}

/** Reactive platform, `null` until detected after mount (keeps SSR output stable). */
export function usePlatform(): Ref<DetectedPlatform | null> {
  const platform = ref<DetectedPlatform | null>(null);
  onMounted(() => {
    void detectPlatform().then((p) => (platform.value = p));
  });
  return platform;
}
