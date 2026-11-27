// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * Terminal color schemes. "Midnight", "Daylight", "Ember" and "Glacier" are originals of this
 * project; the others reproduce the published palettes of their (MIT-licensed) upstream themes.
 */
import type { ITheme } from '@xterm/xterm';

import { findPreset, type PresetId } from '@/lib/presets';

export interface TerminalTheme {
  id: string;
  name: string;
  dark: boolean;
  colors: ITheme;
}

function palette(
  id: string,
  name: string,
  dark: boolean,
  base: { background: string; foreground: string; cursor?: string; selection: string },
  ansi: [string, string, string, string, string, string, string, string],
  bright: [string, string, string, string, string, string, string, string],
): TerminalTheme {
  const [black, red, green, yellow, blue, magenta, cyan, white] = ansi;
  const [bBlack, bRed, bGreen, bYellow, bBlue, bMagenta, bCyan, bWhite] = bright;
  return {
    id,
    name,
    dark,
    colors: {
      background: base.background,
      foreground: base.foreground,
      cursor: base.cursor ?? base.foreground,
      cursorAccent: base.background,
      selectionBackground: base.selection,
      black,
      red,
      green,
      yellow,
      blue,
      magenta,
      cyan,
      white,
      brightBlack: bBlack,
      brightRed: bRed,
      brightGreen: bGreen,
      brightYellow: bYellow,
      brightBlue: bBlue,
      brightMagenta: bMagenta,
      brightCyan: bCyan,
      brightWhite: bWhite,
    },
  };
}

export const TERMINAL_THEMES: TerminalTheme[] = [
  palette(
    'midnight',
    'Midnight',
    true,
    { background: '#0d1017', foreground: '#d8dcea', cursor: '#8b8ff8', selection: '#2a3150' },
    ['#1a1e2b', '#f2687a', '#5fd69b', '#f4c15c', '#6f9cff', '#b58bff', '#4fd0e0', '#c9cee0'],
    ['#4a5270', '#ff8a98', '#84e6b4', '#ffd685', '#93b6ff', '#cbaaff', '#7be2ee', '#f4f6fb'],
  ),
  palette(
    'daylight',
    'Daylight',
    false,
    { background: '#fbfbfd', foreground: '#2a3042', cursor: '#4f46e5', selection: '#d9dcf7' },
    ['#2a3042', '#d23b4f', '#1d8a55', '#a86a00', '#2f5fd0', '#7b3fd1', '#0b8394', '#9aa1b5'],
    ['#5b6276', '#e8566a', '#25a466', '#c58200', '#4a77e6', '#9358e8', '#159cb0', '#c9cdd9'],
  ),
  palette(
    'ember',
    'Ember (production)',
    true,
    { background: '#1a0d0f', foreground: '#f1d9d9', cursor: '#ff6b6b', selection: '#4a1f24' },
    ['#2a1518', '#ff5c5c', '#9ad67a', '#f5b54a', '#7aa2f7', '#e08bd4', '#6fd3cf', '#e6cfcf'],
    ['#5c3338', '#ff8080', '#b5e69a', '#ffcc70', '#9bb9ff', '#f0a8e6', '#93e4e0', '#fff0f0'],
  ),
  palette(
    'glacier',
    'Glacier',
    true,
    { background: '#0b1418', foreground: '#d4e7ee', cursor: '#56d6f0', selection: '#1d3a45' },
    ['#142229', '#ef6f7a', '#5fd3a8', '#e9c46a', '#5aa9ff', '#a98bf5', '#56d6f0', '#c8dbe2'],
    ['#3b5864', '#ff8f98', '#86e3c0', '#f5d78d', '#85c0ff', '#c2a9ff', '#8ae5f6', '#f2fbfe'],
  ),
  palette(
    'dracula',
    'Dracula',
    true,
    { background: '#282a36', foreground: '#f8f8f2', selection: '#44475a' },
    ['#21222c', '#ff5555', '#50fa7b', '#f1fa8c', '#bd93f9', '#ff79c6', '#8be9fd', '#f8f8f2'],
    ['#6272a4', '#ff6e6e', '#69ff94', '#ffffa5', '#d6acff', '#ff92df', '#a4ffff', '#ffffff'],
  ),
  palette(
    'nord',
    'Nord',
    true,
    { background: '#2e3440', foreground: '#d8dee9', selection: '#434c5e' },
    ['#3b4252', '#bf616a', '#a3be8c', '#ebcb8b', '#81a1c1', '#b48ead', '#88c0d0', '#e5e9f0'],
    ['#4c566a', '#bf616a', '#a3be8c', '#ebcb8b', '#81a1c1', '#b48ead', '#8fbcbb', '#eceff4'],
  ),
  palette(
    'gruvbox-dark',
    'Gruvbox Dark',
    true,
    { background: '#282828', foreground: '#ebdbb2', selection: '#504945' },
    ['#282828', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#a89984'],
    ['#928374', '#fb4934', '#b8bb26', '#fabd2f', '#83a598', '#d3869b', '#8ec07c', '#ebdbb2'],
  ),
  palette(
    'gruvbox-light',
    'Gruvbox Light',
    false,
    { background: '#fbf1c7', foreground: '#3c3836', selection: '#ebdbb2' },
    ['#fbf1c7', '#cc241d', '#98971a', '#d79921', '#458588', '#b16286', '#689d6a', '#7c6f64'],
    ['#928374', '#9d0006', '#79740e', '#b57614', '#076678', '#8f3f71', '#427b58', '#3c3836'],
  ),
  palette(
    'catppuccin-mocha',
    'Catppuccin Mocha',
    true,
    { background: '#1e1e2e', foreground: '#cdd6f4', cursor: '#f5e0dc', selection: '#45475a' },
    ['#45475a', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#bac2de'],
    ['#585b70', '#f38ba8', '#a6e3a1', '#f9e2af', '#89b4fa', '#f5c2e7', '#94e2d5', '#a6adc8'],
  ),
  palette(
    'catppuccin-latte',
    'Catppuccin Latte',
    false,
    { background: '#eff1f5', foreground: '#4c4f69', cursor: '#dc8a78', selection: '#ccd0da' },
    ['#5c5f77', '#d20f39', '#40a02b', '#df8e1d', '#1e66f5', '#ea76cb', '#179299', '#acb0be'],
    ['#6c6f85', '#d20f39', '#40a02b', '#df8e1d', '#1e66f5', '#ea76cb', '#179299', '#bcc0cc'],
  ),
  palette(
    'tokyo-night',
    'Tokyo Night',
    true,
    { background: '#1a1b26', foreground: '#c0caf5', selection: '#33467c' },
    ['#15161e', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#a9b1d6'],
    ['#414868', '#f7768e', '#9ece6a', '#e0af68', '#7aa2f7', '#bb9af7', '#7dcfff', '#c0caf5'],
  ),
  palette(
    'tokyo-day',
    'Tokyo Night Day',
    false,
    { background: '#e1e2e7', foreground: '#3760bf', selection: '#b6bfe2' },
    ['#e9e9ed', '#f52a65', '#587539', '#8c6c3e', '#2e7de9', '#9854f1', '#007197', '#6172b0'],
    ['#a1a6c5', '#f52a65', '#587539', '#8c6c3e', '#2e7de9', '#9854f1', '#007197', '#3760bf'],
  ),
  palette(
    'solarized-dark',
    'Solarized Dark',
    true,
    { background: '#002b36', foreground: '#839496', selection: '#073642' },
    ['#073642', '#dc322f', '#859900', '#b58900', '#268bd2', '#d33682', '#2aa198', '#eee8d5'],
    ['#002b36', '#cb4b16', '#586e75', '#657b83', '#839496', '#6c71c4', '#93a1a1', '#fdf6e3'],
  ),
  palette(
    'solarized-light',
    'Solarized Light',
    false,
    { background: '#fdf6e3', foreground: '#657b83', selection: '#eee8d5' },
    ['#073642', '#dc322f', '#859900', '#b58900', '#268bd2', '#d33682', '#2aa198', '#eee8d5'],
    ['#002b36', '#cb4b16', '#586e75', '#657b83', '#839496', '#6c71c4', '#93a1a1', '#fdf6e3'],
  ),
  palette(
    'one-dark',
    'One Dark',
    true,
    { background: '#282c34', foreground: '#abb2bf', selection: '#3e4451' },
    ['#282c34', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#abb2bf'],
    ['#5c6370', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#ffffff'],
  ),
  palette(
    'one-light',
    'One Light',
    false,
    { background: '#fafafa', foreground: '#383a42', selection: '#e5e5e6' },
    ['#383a42', '#e45649', '#50a14f', '#c18401', '#0184bc', '#a626a4', '#0997b3', '#fafafa'],
    ['#4f525e', '#e06c75', '#98c379', '#e5c07b', '#61afef', '#c678dd', '#56b6c2', '#ffffff'],
  ),
  palette(
    'rose-pine',
    'Rosé Pine',
    true,
    { background: '#191724', foreground: '#e0def4', selection: '#403d52' },
    ['#26233a', '#eb6f92', '#31748f', '#f6c177', '#9ccfd8', '#c4a7e7', '#ebbcba', '#e0def4'],
    ['#6e6a86', '#eb6f92', '#31748f', '#f6c177', '#9ccfd8', '#c4a7e7', '#ebbcba', '#e0def4'],
  ),
  palette(
    'rose-pine-dawn',
    'Rosé Pine Dawn',
    false,
    { background: '#faf4ed', foreground: '#575279', selection: '#dfdad9' },
    ['#f2e9e1', '#b4637a', '#286983', '#ea9d34', '#56949f', '#907aa9', '#d7827e', '#575279'],
    ['#9893a5', '#b4637a', '#286983', '#ea9d34', '#56949f', '#907aa9', '#d7827e', '#575279'],
  ),
  palette(
    'kanagawa',
    'Kanagawa Wave',
    true,
    { background: '#1f1f28', foreground: '#dcd7ba', selection: '#2d4f67' },
    ['#16161d', '#c34043', '#76946a', '#c0a36e', '#7e9cd8', '#957fb8', '#6a9589', '#c8c093'],
    ['#727169', '#e82424', '#98bb6c', '#e6c384', '#7fb4ca', '#938aa9', '#7aa89f', '#dcd7ba'],
  ),
  palette(
    'everforest-dark',
    'Everforest Dark',
    true,
    { background: '#2d353b', foreground: '#d3c6aa', selection: '#475258' },
    ['#475258', '#e67e80', '#a7c080', '#dbbc7f', '#7fbbb3', '#d699b6', '#83c092', '#d3c6aa'],
    ['#475258', '#e67e80', '#a7c080', '#dbbc7f', '#7fbbb3', '#d699b6', '#83c092', '#d3c6aa'],
  ),
  palette(
    'everforest-light',
    'Everforest Light',
    false,
    { background: '#fdf6e3', foreground: '#5c6a72', selection: '#e6e2cc' },
    ['#5c6a72', '#f85552', '#8da101', '#dfa000', '#3a94c5', '#df69ba', '#35a77c', '#dfddc8'],
    ['#5c6a72', '#f85552', '#8da101', '#dfa000', '#3a94c5', '#df69ba', '#35a77c', '#dfddc8'],
  ),
  palette(
    'material',
    'Material Dark',
    true,
    { background: '#212121', foreground: '#eeffff', selection: '#404040' },
    ['#212121', '#f07178', '#c3e88d', '#ffcb6b', '#82aaff', '#c792ea', '#89ddff', '#eeffff'],
    ['#545454', '#f07178', '#c3e88d', '#ffcb6b', '#82aaff', '#c792ea', '#89ddff', '#ffffff'],
  ),
  palette(
    'ayu-dark',
    'Ayu Dark',
    true,
    { background: '#0b0e14', foreground: '#bfbdb6', cursor: '#e6b450', selection: '#273747' },
    ['#1e232b', '#ea6c73', '#7fd962', '#f9af4f', '#53bdfa', '#cda1fa', '#90e1c6', '#c7c7c7'],
    ['#686868', '#f07178', '#aad94c', '#ffb454', '#59c2ff', '#d2a6ff', '#95e6cb', '#ffffff'],
  ),
  palette(
    'ayu-light',
    'Ayu Light',
    false,
    { background: '#fcfcfc', foreground: '#5c6166', cursor: '#ffaa33', selection: '#d1e4f4' },
    ['#000000', '#ea6c6d', '#6cbf43', '#eca944', '#3199e1', '#9e75c7', '#46ba94', '#bababa'],
    ['#686868', '#f07171', '#86b300', '#f2ae49', '#399ee6', '#a37acc', '#4cbf99', '#d1d1d1'],
  ),
];

export const DEFAULT_DARK_THEME = 'midnight';
export const DEFAULT_LIGHT_THEME = 'daylight';

export function findTheme(
  id: string | null | undefined,
  appScheme: 'dark' | 'light',
): TerminalTheme {
  const fallback = appScheme === 'light' ? DEFAULT_LIGHT_THEME : DEFAULT_DARK_THEME;
  const found =
    TERMINAL_THEMES.find((t) => t.id === id) ?? TERMINAL_THEMES.find((t) => t.id === fallback);
  if (found) return found;
  const [first] = TERMINAL_THEMES;
  if (!first) throw new Error('no terminal themes');
  return first;
}

export const TERMINAL_FONTS = [
  'JetBrains Mono',
  'Fira Code',
  'Cascadia Code',
  'Source Code Pro',
  'Hack',
  'Iosevka',
  'SF Mono',
  'Menlo',
  'Consolas',
  'DejaVu Sans Mono',
  'Ubuntu Mono',
] as const;

/**
 * The default themes follow the app: the dark one takes the window background, and both take
 * the colour preset for the cursor and selection. Other themes are used as they are.
 */
export function themeColors(
  id: string | null | undefined,
  appScheme: 'dark' | 'light',
  preset: PresetId,
): TerminalTheme['colors'] {
  const theme = findTheme(id, appScheme);
  if (theme.id !== DEFAULT_DARK_THEME && theme.id !== DEFAULT_LIGHT_THEME) return theme.colors;
  const p = findPreset(preset);
  const accent = theme.dark ? p.swatch : p.light;
  return {
    ...theme.colors,
    ...(theme.dark ? { background: p.bg } : {}),
    cursor: accent,
    cursorAccent: theme.dark ? p.bg : '#ffffff',
    selectionBackground: `${accent}${theme.dark ? '40' : '30'}`,
  };
}
