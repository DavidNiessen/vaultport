// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Colour presets of the "Pulse" design. The colours themselves live in styles/index.css under
 * `[data-preset='…']`; the swatches here only drive the picker in Settings.
 */
export const PRESETS = [
  { id: 'glacier', swatch: '#6fe3d9', glow: ['#46bec8', '#5078dc'] },
  { id: 'iris', swatch: '#b3a6ff', glow: ['#826eff', '#5a8ce6'] },
  { id: 'ember', swatch: '#ff8a4c', glow: ['#ff783c', '#be465a'] },
  { id: 'rose', swatch: '#ff8fbd', glow: ['#ff64a0', '#9650c8'] },
  { id: 'moss', swatch: '#93d99a', glow: ['#6ec878', '#3c968c'] },
  { id: 'mono', swatch: '#e8e8ec', glow: ['#a0a0af', '#6e6e82'] },
] as const;

export type PresetId = (typeof PRESETS)[number]['id'];

export const DEFAULT_PRESET: PresetId = 'glacier';

export function isPreset(value: unknown): value is PresetId {
  return PRESETS.some((p) => p.id === value);
}
