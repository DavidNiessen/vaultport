// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';

import {
  SHORTCUTS,
  actionFor,
  bindingFromEvent,
  buildKeymap,
  conflicts,
  matches,
  parseBinding,
} from './shortcuts';

function key(
  k: string,
  code: string,
  mods: Partial<Pick<KeyboardEvent, 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>> = {},
): KeyboardEvent {
  return new KeyboardEvent('keydown', { key: k, code, ...mods });
}

describe('shortcuts', () => {
  it('maps Mod to Cmd on Apple platforms and Ctrl elsewhere', () => {
    expect(matches(key('t', 'KeyT', { metaKey: true }), 'Mod+T', true)).toBe(true);
    expect(matches(key('t', 'KeyT', { ctrlKey: true }), 'Mod+T', true)).toBe(false);
    expect(matches(key('t', 'KeyT', { ctrlKey: true }), 'Mod+T', false)).toBe(true);
    expect(matches(key('t', 'KeyT', { metaKey: true }), 'Mod+T', false)).toBe(false);
    // Ctrl is the Control key on every platform.
    expect(matches(key('r', 'KeyR', { ctrlKey: true }), 'Ctrl+R', true)).toBe(true);
    expect(matches(key('r', 'KeyR', { ctrlKey: true }), 'Ctrl+R', false)).toBe(true);
  });

  it('requires the exact modifiers', () => {
    expect(matches(key('W', 'KeyW', { ctrlKey: true, shiftKey: true }), 'Mod+W', false)).toBe(
      false,
    );
    expect(matches(key('W', 'KeyW', { ctrlKey: true, shiftKey: true }), 'Mod+Shift+W', false)).toBe(
      true,
    );
    expect(matches(key('w', 'KeyW', { ctrlKey: true, altKey: true }), 'Mod+W', false)).toBe(false);
  });

  it('matches by physical key on other layouts', () => {
    // A German or Russian layout still has W where QWERTY has it.
    expect(matches(key('ц', 'KeyW', { ctrlKey: true }), 'Mod+W', false)).toBe(true);
    expect(
      matches(key('}', 'BracketRight', { metaKey: true, shiftKey: true }), 'Mod+Shift+]', true),
    ).toBe(true);
  });

  it('understands named and shifted keys', () => {
    expect(matches(key(' ', 'Space', { ctrlKey: true }), 'Ctrl+Space', false)).toBe(true);
    expect(matches(key('ArrowUp', 'ArrowUp', { metaKey: true }), 'Mod+Up', true)).toBe(true);
    expect(matches(key('+', 'Equal', { ctrlKey: true, shiftKey: true }), 'Mod++', false)).toBe(
      true,
    );
    expect(matches(key('=', 'Equal', { ctrlKey: true }), 'Mod+=', false)).toBe(true);
    expect(matches(key('Tab', 'Tab', { ctrlKey: true }), 'Ctrl+Tab', true)).toBe(true);
    expect(matches(key('`', 'Backquote', { ctrlKey: true }), 'Ctrl+`', false)).toBe(true);
  });

  it('rejects malformed bindings', () => {
    expect(parseBinding('Hyper+T')).toBeNull();
    expect(parseBinding('Ctrl+')).toBeNull();
    expect(parseBinding('Shift')).toBeNull();
    expect(parseBinding('Mod++')?.key).toBe('+');
  });

  it('applies overrides and finds actions by scope', () => {
    const keymap = buildKeymap(false, { newTab: ['Ctrl+Shift+T'], closeTab: [] });
    expect(keymap.newTab).toEqual(['Ctrl+Shift+T']);
    expect(keymap.closeTab).toEqual([]);
    const e = key('T', 'KeyT', { ctrlKey: true, shiftKey: true });
    expect(actionFor(e, keymap, false, 'global')).toBe('newTab');
    expect(actionFor(key('w', 'KeyW', { ctrlKey: true }), keymap, false, 'global')).toBeNull();
    expect(actionFor(key('r', 'KeyR', { ctrlKey: true }), keymap, false, 'terminal')).toBe(
      'historySearch',
    );
    expect(actionFor(key('r', 'KeyR', { ctrlKey: true }), keymap, false, 'global')).toBeNull();
  });

  it('opens and closes tabs with Ctrl+T / Ctrl+W, and Cmd on macOS', () => {
    for (const apple of [false, true]) {
      const keymap = buildKeymap(apple);
      const mod = apple ? { metaKey: true } : { ctrlKey: true };
      expect(actionFor(key('t', 'KeyT', mod), keymap, apple, 'global')).toBe('newTab');
      expect(actionFor(key('w', 'KeyW', mod), keymap, apple, 'global')).toBe('closeTab');
      expect(actionFor(key('T', 'KeyT', { ...mod, shiftKey: true }), keymap, apple, 'global')).toBe(
        'reopenTab',
      );
    }
  });

  it('records bindings from key presses', () => {
    expect(bindingFromEvent(key('t', 'KeyT', { metaKey: true, shiftKey: true }), true)).toBe(
      'Mod+Shift+T',
    );
    expect(bindingFromEvent(key('r', 'KeyR', { ctrlKey: true }), true)).toBe('Ctrl+R');
    expect(bindingFromEvent(key('ArrowUp', 'ArrowUp', { ctrlKey: true }), false)).toBe('Mod+Up');
    expect(bindingFromEvent(key('Control', 'ControlLeft', { ctrlKey: true }), false)).toBeNull();
    // Recorded bindings match the event they were recorded from.
    const e = key('k', 'KeyK', { ctrlKey: true, altKey: true });
    const b = bindingFromEvent(e, false);
    expect(b && matches(e, b, false)).toBe(true);
  });

  it('has no conflicting defaults', () => {
    for (const apple of [false, true]) {
      expect([...conflicts(buildKeymap(apple)).keys()]).toEqual([]);
    }
    expect(new Set(SHORTCUTS.map((s) => s.id)).size).toBe(SHORTCUTS.length);
  });
});
