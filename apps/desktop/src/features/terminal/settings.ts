// SPDX-License-Identifier: GPL-3.0-or-later
/** Terminal preferences stored (encrypted) in the vault settings document `terminal`. */
export interface TerminalSettings {
  theme: string;
  fontFamily: string;
  fontSize: number;
  cursorStyle: 'block' | 'bar' | 'underline';
  cursorBlink: boolean;
  scrollback: number;
  brightBold: boolean;
  copyOnSelect: boolean;
  rightClickPaste: boolean;
  optionAsMeta: boolean;
  highlightKeywords: boolean;
  autoReconnect: boolean;
  remoteTitles: boolean;
  bell: 'none' | 'visual' | 'sound';
  osc52: 'allow' | 'ask' | 'deny';
  termType: string;
  localShell: string;
  shellIntegration: boolean;
  autocomplete: boolean;
  importHistory: boolean;
  detectOs: boolean;
}

export const DEFAULT_TERMINAL_SETTINGS: TerminalSettings = {
  theme: 'midnight',
  fontFamily: 'JetBrains Mono',
  fontSize: 14,
  cursorStyle: 'block',
  cursorBlink: true,
  scrollback: 10_000,
  brightBold: true,
  copyOnSelect: false,
  rightClickPaste: false,
  optionAsMeta: true,
  highlightKeywords: true,
  autoReconnect: true,
  remoteTitles: true,
  bell: 'visual',
  osc52: 'ask',
  termType: 'xterm-256color',
  localShell: '',
  shellIntegration: false,
  autocomplete: true,
  importHistory: false,
  detectOs: true,
};
