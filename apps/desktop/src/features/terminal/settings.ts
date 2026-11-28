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
  /** `auto`: the suggestion list opens while typing; `tab`: only on Tab / Ctrl+Space. */
  suggestMode: 'auto' | 'tab';
  /** The rest of the best history match, dimmed after the cursor. */
  ghostText: boolean;
  /** Colour the command line while typing. */
  syntaxHighlight: boolean;
  /** Separators, exit status and duration per command (needs shell integration). */
  commandBlocks: boolean;
  /** Tell when a command that ran at least this many seconds finishes in the background. */
  notifyAfterSeconds: number;
  /** Load shell integration into SSH sessions automatically (bash and zsh). */
  sshIntegration: 'off' | 'auto';
  /** `# what you want` at the prompt asks the local AI (when it is on). */
  aiHashPrompt: boolean;
  /** Reopen the terminal tabs that were open, after unlocking. */
  restoreTabs: boolean;
  /** Working directory, git branch and last exit status under the terminal. */
  contextBar: boolean;
  /** Extra output highlighting rules. */
  highlightRules: HighlightRule[];
}

/** A user-defined output highlighting rule. */
export interface HighlightRule {
  /** Regular expression (JavaScript syntax). */
  pattern: string;
  /** `#rrggbb`. */
  color: string;
  caseSensitive: boolean;
}

export const DEFAULT_TERMINAL_SETTINGS: TerminalSettings = {
  theme: 'pulse',
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
  shellIntegration: true,
  autocomplete: true,
  importHistory: false,
  detectOs: true,
  suggestMode: 'auto',
  ghostText: true,
  syntaxHighlight: true,
  commandBlocks: true,
  notifyAfterSeconds: 10,
  sshIntegration: 'off',
  aiHashPrompt: true,
  contextBar: true,
  restoreTabs: false,
  highlightRules: [],
};
