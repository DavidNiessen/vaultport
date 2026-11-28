// SPDX-License-Identifier: GPL-3.0-or-later
/**
 * macOS line-editing keys, as in Terminal.app, iTerm2 and Warp: ⌘←/⌘→ jump to the start or end
 * of the line, ⌥←/⌥→ move by word, ⌘⌫ deletes to the start of the line and ⌥⌫ deletes a
 * word. The sequences are the emacs-mode bindings that bash, zsh and fish understand.
 */
export function macEditingKeys(e: KeyboardEvent): string | null {
  if (e.type !== 'keydown' || e.ctrlKey || e.shiftKey) return null;
  if (e.metaKey && !e.altKey) {
    switch (e.key) {
      case 'ArrowLeft':
        return '\x01'; // beginning-of-line
      case 'ArrowRight':
        return '\x05'; // end-of-line
      case 'Backspace':
        return '\x15'; // unix-line-discard
      default:
        return null;
    }
  }
  if (e.altKey && !e.metaKey) {
    switch (e.key) {
      case 'ArrowLeft':
        return '\x1bb'; // backward-word
      case 'ArrowRight':
        return '\x1bf'; // forward-word
      case 'Backspace':
        return '\x17'; // unix-word-rubout
      default:
        return null;
    }
  }
  return null;
}
