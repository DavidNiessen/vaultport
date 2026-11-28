---
title: Keyboard shortcuts
description: Default keyboard shortcuts for tabs, the command palette, the terminal (search, history, AI, command blocks, zoom) and workspaces, and how to change them.
---

# Keyboard shortcuts

<kbd>Mod</kbd> is <kbd>⌘ Command</kbd> on macOS and iPadOS and <kbd>Ctrl</kbd> on Windows, Linux and Android. <kbd>Ctrl</kbd> is the Control key everywhere.

## Changing shortcuts

**Settings → Shortcuts** lists every action with its shortcuts. Click **+** and press a key combination to add one, hover a shortcut and click **×** to remove it, or reset an action (or all of them) to the defaults. Shortcuts used by two actions are marked. Your shortcuts are stored in the vault and sync with it.

Terminal shortcuts take the key away from the shell. If you need a key in the shell — say <kbd>Ctrl</kbd>+<kbd>R</kbd> for its own history search, or <kbd>Ctrl</kbd>+<kbd>W</kbd> to delete a word on Linux — remove that shortcut.

## Tabs

| Action                | Windows / Linux                                                                  | macOS                                                                                                                    |
| --------------------- | -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| New terminal tab      | <kbd>Ctrl</kbd>+<kbd>T</kbd>                                                     | <kbd>⌘</kbd>+<kbd>T</kbd>                                                                                                |
| Connect to a host…    | <kbd>Ctrl</kbd>+<kbd>O</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>      | <kbd>⌘</kbd>+<kbd>O</kbd>, <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>N</kbd>                                                        |
| Close tab             | <kbd>Ctrl</kbd>+<kbd>W</kbd>, <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd>      | <kbd>⌘</kbd>+<kbd>W</kbd>, <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>W</kbd>                                                        |
| Reopen closed tab     | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd>                                    | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>T</kbd>                                                                                   |
| Duplicate tab         | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>                                    | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>D</kbd>                                                                                   |
| Rename tab            | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd>, or double-click it                | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>R</kbd>, or double-click it                                                               |
| Next tab              | <kbd>Ctrl</kbd>+<kbd>Tab</kbd>, <kbd>Ctrl</kbd>+<kbd>PgDn</kbd>                  | <kbd>⌃</kbd>+<kbd>Tab</kbd>, <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>]</kbd>, <kbd>⌘</kbd>+<kbd>⌥</kbd>+<kbd>→</kbd>              |
| Previous tab          | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Tab</kbd>, <kbd>Ctrl</kbd>+<kbd>PgUp</kbd> | <kbd>⌃</kbd>+<kbd>⇧</kbd>+<kbd>Tab</kbd>, <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>[</kbd>, <kbd>⌘</kbd>+<kbd>⌥</kbd>+<kbd>←</kbd> |
| Move tab left / right | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PgUp</kbd> / <kbd>PgDn</kbd>               | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>⌥</kbd>+<kbd>←</kbd> / <kbd>→</kbd>                                                       |
| Go to tab 1–8 / last  | <kbd>Ctrl</kbd>+<kbd>1</kbd>…<kbd>8</kbd> / <kbd>9</kbd> (also <kbd>Alt</kbd>)   | <kbd>⌘</kbd>+<kbd>1</kbd>…<kbd>8</kbd> / <kbd>9</kbd>                                                                    |

The pinned Vault and SFTP tabs count as tabs 1 and 2. **New terminal tab** opens a local shell (in the directory of the current local tab) where the device has one, and the connection picker on phones and tablets.

## App

| Action              | Shortcut                                                                  |
| ------------------- | ------------------------------------------------------------------------- |
| Command palette     | <kbd>Mod</kbd>+<kbd>K</kbd>, <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd> |
| Jump to tab or host | <kbd>Mod</kbd>+<kbd>J</kbd>                                               |
| Toggle side panel   | <kbd>Mod</kbd>+<kbd>.</kbd>                                               |
| Settings            | <kbd>Mod</kbd>+<kbd>,</kbd>                                               |
| Lock vault          | <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd>                              |

## Terminal

| Action                        | Windows / Linux                                              | macOS                                                   |
| ----------------------------- | ------------------------------------------------------------ | ------------------------------------------------------- |
| Search command history        | <kbd>Ctrl</kbd>+<kbd>R</kbd>                                 | <kbd>⌃</kbd>+<kbd>R</kbd>                               |
| Show suggestions              | <kbd>Ctrl</kbd>+<kbd>Space</kbd>                             | <kbd>⌃</kbd>+<kbd>Space</kbd>                           |
| Ask AI for a command          | <kbd>Ctrl</kbd>+<kbd>&#96;</kbd>                                 | <kbd>⌃</kbd>+<kbd>&#96;</kbd>                               |
| Command editor                | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd>                | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>E</kbd>                  |
| Previous / next command       | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd> | <kbd>⌘</kbd>+<kbd>↑</kbd> / <kbd>↓</kbd>                |
| Copy last output              | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>                | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>O</kbd>                  |
| Search in terminal            | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>                | <kbd>⌘</kbd>+<kbd>F</kbd>                               |
| Clear terminal                | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>K</kbd>                | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>K</kbd>                  |
| Larger / smaller / reset text | <kbd>Ctrl</kbd>+<kbd>=</kbd> / <kbd>-</kbd> / <kbd>0</kbd>   | <kbd>⌘</kbd>+<kbd>=</kbd> / <kbd>-</kbd> / <kbd>0</kbd> |
| Copy selection                | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>                | <kbd>⌘</kbd>+<kbd>C</kbd>                               |
| Paste                         | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>                | <kbd>⌘</kbd>+<kbd>V</kbd>                               |

While typing a command:

| Key                                                               | Action                                     |
| ----------------------------------------------------------------- | ------------------------------------------ |
| <kbd>Tab</kbd>                                                    | Insert the selected suggestion             |
| <kbd>↑</kbd> / <kbd>↓</kbd>                                       | Choose a suggestion (while the list shows) |
| <kbd>→</kbd> or <kbd>End</kbd>                                    | Accept the grey inline suggestion          |
| <kbd>Ctrl</kbd>+<kbd>→</kbd> (<kbd>⌥</kbd>+<kbd>→</kbd> on macOS) | Accept its next word                       |
| `# what you want` + <kbd>Enter</kbd>                              | Ask the local AI for a command             |

macOS also has the usual line editing keys: <kbd>⌘</kbd>+<kbd>←</kbd>/<kbd>→</kbd> (start / end of line), <kbd>⌥</kbd>+<kbd>←</kbd>/<kbd>→</kbd> (previous / next word), <kbd>⌘</kbd>+<kbd>⌫</kbd> (delete to the start of the line) and <kbd>⌥</kbd>+<kbd>⌫</kbd> (delete a word).

Plain <kbd>Ctrl</kbd>+<kbd>C</kbd> is always sent to the remote program (to interrupt it), which is why copying uses <kbd>Shift</kbd> outside macOS. You can also enable **Copy on select** and **Right-click to paste** in **Settings → Terminal**. On macOS, **Use Option as Meta key** (on by default) sends <kbd>⌥ Option</kbd> combinations as Meta/Alt to the shell.

## Workspaces

| Action               | Shortcut                                     |
| -------------------- | -------------------------------------------- |
| Toggle broadcast     | <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> |
| Toggle split / focus | <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>M</kbd> |

## Mouse and touch

- **Middle-click** a tab to close it; **drag** tabs to reorder them; **right-click** a tab for more.
- **Double-click** a tab to rename it, or the title bar to maximize or restore the window.
- **Click** a command's exit-status badge for its actions.
- **Double-click** a port-forwarding rule to start or stop it.
- <kbd>Mod</kbd>+click a link in the terminal to open it (you are asked first).
