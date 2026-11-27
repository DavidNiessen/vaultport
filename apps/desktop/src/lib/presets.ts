// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Colour presets of the "Pulse" design. The colours themselves live in styles/index.css under
 * `[data-preset='…']`. The values here drive the picker in Settings and the default terminal
 * themes, which xterm needs as plain colours: `swatch` is the dark-theme accent, `light` the
 * light-theme accent and `bg` the dark window background. Keep them in sync with the CSS.
 */
export const PRESETS = [
  {
    id: 'glacier',
    swatch: '#6fe3d9',
    glow: ['#46bec8', '#5078dc'],
    bg: '#0b0f12',
    light: '#0b7d74',
  },
  { id: 'iris', swatch: '#b3a6ff', glow: ['#826eff', '#5a8ce6'], bg: '#0d0c12', light: '#5b47e0' },
  { id: 'ember', swatch: '#ff8a4c', glow: ['#ff783c', '#be465a'], bg: '#100d0c', light: '#c2410c' },
  { id: 'rose', swatch: '#ff8fbd', glow: ['#ff64a0', '#9650c8'], bg: '#110c0f', light: '#be185d' },
  { id: 'moss', swatch: '#93d99a', glow: ['#6ec878', '#3c968c'], bg: '#0c100d', light: '#2f7d3a' },
  { id: 'mono', swatch: '#e8e8ec', glow: ['#a0a0af', '#6e6e82'], bg: '#0e0e10', light: '#18181b' },
] as const;

export type PresetId = (typeof PRESETS)[number]['id'];

export const DEFAULT_PRESET: PresetId = 'glacier';

export function isPreset(value: unknown): value is PresetId {
  return PRESETS.some((p) => p.id === value);
}

export function findPreset(id: PresetId) {
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0];
}
