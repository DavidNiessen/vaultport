---
title: The terminal
description: Vaultport's terminal for SSH and local shells — tabs, autocomplete with commands installed on the machine, inline suggestions, syntax highlighting, command blocks, history search, the command editor and on-device AI.
---

# The terminal

Vaultport is a full terminal app, for SSH, Mosh, Telnet and serial sessions as well as local shells. The features on this page work the same over SSH and on your own computer: what they need from the remote machine is read over the session's own connection, and nothing is ever run on the server for them.

## Tabs

| Action                   | Windows / Linux                                                      | macOS                                                                            |
| ------------------------ | -------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| New terminal tab         | <kbd>Ctrl</kbd>+<kbd>T</kbd>                                         | <kbd>⌘</kbd>+<kbd>T</kbd>                                                        |
| Close tab                | <kbd>Ctrl</kbd>+<kbd>W</kbd>                                         | <kbd>⌘</kbd>+<kbd>W</kbd>                                                        |
| Reopen closed tab        | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>T</kbd>                        | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>T</kbd>                                           |
| Connect to a host…       | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>                        | <kbd>⌘</kbd>+<kbd>O</kbd>                                                        |
| Split pane               | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>                        | <kbd>⌘</kbd>+<kbd>D</kbd>                                                        |
| Next / previous pane     | <kbd>Ctrl</kbd>+<kbd>Alt</kbd>+<kbd>→</kbd> / <kbd>←</kbd>           | <kbd>⌘</kbd>+<kbd>]</kbd> / <kbd>[</kbd>                                         |
| Rename tab               | <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd>                        | <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>R</kbd>                                           |
| Next / previous tab      | <kbd>Ctrl</kbd>+<kbd>Tab</kbd>, <kbd>Ctrl</kbd>+<kbd>PgDn/PgUp</kbd> | <kbd>⌃</kbd>+<kbd>Tab</kbd>, <kbd>⌘</kbd>+<kbd>⇧</kbd>+<kbd>]</kbd>/<kbd>[</kbd> |
| Go to tab 1–8, last tab¹ | <kbd>Ctrl</kbd>+<kbd>1</kbd>…<kbd>8</kbd>, <kbd>9</kbd>              | <kbd>⌘</kbd>+<kbd>1</kbd>…<kbd>8</kbd>, <kbd>9</kbd>                             |

¹ The pinned Vault and SFTP tabs are 1 and 2. On Windows and Linux, <kbd>Alt</kbd>+<kbd>1</kbd>…<kbd>9</kbd> works too.

- **New terminal tab** opens a local shell on desktop, in the directory of the local tab you are in. On phones and tablets, where there is no local shell, it opens the connection picker.
- **Split** a tab to see two sessions side by side: the new pane is another connection to the same host (or a local shell in the same directory). In a split, <kbd>Mod</kbd>+<kbd>W</kbd> closes only the focused pane, and when one pane is left it becomes a normal tab again. Splits support up to 16 panes, [broadcast input](/guide/workspaces) and focus mode.
- **Duplicate** (tab menu or command palette) opens another connection to the same host (or another local shell in the same directory) right next to the tab.
- **Double-click** a tab (or press <kbd>F2</kbd> on it) to rename it; a renamed tab keeps its name even when a program sets the title. Clear the name to hand the title back.
- Closing a tab or pane in which a command is still running (known with [shell integration](#shell-integration)) asks first, so a stray <kbd>Ctrl</kbd>+<kbd>W</kbd> does not end a long job.
- **Right-click** a tab for duplicate, rename, close, close other tabs, close tabs to the right and reopen closed tab. Up to 20 closed terminal tabs are remembered until you quit.
- **Reopen tabs on start** (Settings → Terminal) reopens your terminal tabs and splits after you unlock. Only host references and directories are saved, encrypted in this device's vault, and never synced.

::: tip Ctrl+W and Ctrl+T in the shell
On Windows and Linux, <kbd>Ctrl</kbd>+<kbd>W</kbd> (delete word) and <kbd>Ctrl</kbd>+<kbd>T</kbd> (swap characters) no longer reach the shell (closing a tab with a running command asks first), as in most terminal apps with tabs. If you need them there, remove or change the shortcuts in **Settings → Shortcuts** — every shortcut can be rebound, and <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> closes tabs as well.
:::

## Autocomplete

Suggestions appear at the cursor while you type at the prompt. They come from Vaultport's deterministic completion engine (no AI):

- **Commands installed on the machine.** Vaultport lists the directories on `PATH` — plus the usual places such as `/usr/local/bin`, Homebrew, `~/.local/bin`, `~/.cargo/bin` — and knows the shell builtins. On a server it does this over an SFTP channel of your SSH connection, in the background, once every few minutes.
- **Subcommands, options and values** of about 300 tools: `git`, `docker` and `docker compose`, `kubectl`, `systemctl`, `npm`/`pnpm`/`yarn`/`bun`, `cargo`, `go`, `gh`, `aws`, `terraform`, `brew`, macOS tools such as `defaults`, `launchctl` and `diskutil`, and many more. Typos are forgiven: `git chkout` finds `checkout`, `--fwl` finds `--force-with-lease`.
- **Files and folders** in the shell's working directory, on your computer or on the server.
- **Values from the machine**: git branches, tags and remotes of the repository you are in; `npm run` scripts, `make` and `just` targets; users and groups; SSH hosts from `~/.ssh/config`, `known_hosts`, `/etc/hosts` and your saved hosts; systemd units, init scripts and launchd jobs; network interfaces; signals; environment variables after `$`.
- Your **command history** for the host and your **snippets**.

| Key                                        | Action                            |
| ------------------------------------------ | --------------------------------- |
| <kbd>↑</kbd> / <kbd>↓</kbd>                | Choose a suggestion               |
| <kbd>Tab</kbd>                             | Insert it                         |
| <kbd>→</kbd> or <kbd>End</kbd>             | Accept the grey inline suggestion |
| <kbd>Ctrl</kbd>+<kbd>→</kbd> (⌥→ on macOS) | Accept its next word              |
| <kbd>Ctrl</kbd>+<kbd>Space</kbd>           | Show suggestions now              |
| <kbd>Esc</kbd>                             | Hide the list                     |

**Inline suggestions** show the rest of the best matching command from your history in grey after the cursor, like fish and Warp. In **Settings → Terminal → Suggestions** you can choose to see the list only when you press <kbd>Tab</kbd> (then <kbd>Tab</kbd> goes to the shell when there is nothing to suggest), and turn inline suggestions off.

## Syntax highlighting

The command line is coloured while you type: commands, options, strings, variables, operators, keywords and comments. A command that is not installed on the machine — and that you have not run there before — turns **red** once you finish typing it, so typos such as `gti status` stand out before you press <kbd>Enter</kbd>. The colours come from your terminal theme and stay on the lines that ran.

Output is highlighted too: errors, warnings, IP and MAC addresses, dates, UUIDs, commit hashes and HTTP status codes. Add your own patterns with colours under **Settings → Terminal → Custom output highlighting**, for example `prod-\w+` in red.

## Command blocks

With [shell integration](#shell-integration), every command and its output form a **block**:

- a thin line separates commands; a badge at the right shows the **exit status** and **how long it took** — green for success, red for failure (failures also show in the scrollbar);
- **click the badge** to copy the command, its output or both, run it again, let the [local AI](/guide/local-ai) suggest a fix, or copy a ready-made prompt for your [AI agent](/guide/ai-agents);
- <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> (<kbd>⌘</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> on macOS) jumps between commands, and <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> copies the output of the last one;
- while you scroll through long output, the command it belongs to stays visible at the top — click it to jump there;
- when a command that ran 10 seconds or longer finishes while you are in another tab or app, Vaultport tells you, with a link back to the tab. Change the threshold (or turn it off with `0`) in **Settings → Terminal → Command blocks**.

## Mistyped commands

When a command fails because it does not exist, Vaultport offers the closest installed command — `gti status` → `git status` — with **Run** and **Edit** buttons. The rest of the line is kept.

## Context bar

A slim bar under the terminal shows the working directory (click it to copy the path), the **git branch** (or commit, when detached) and the exit status and duration of the last command. Turn it off in **Settings → Terminal → Command blocks**.

## History search

<kbd>Ctrl</kbd>+<kbd>R</kbd> opens a search over every command you ran on this host, newest and most frequent first, matching words in any order and fuzzy abbreviations. <kbd>Enter</kbd> puts the command on the prompt to edit, <kbd>Mod</kbd>+<kbd>Enter</kbd> runs it. While a command or a full-screen program runs, <kbd>Ctrl</kbd>+<kbd>R</kbd> goes to it as usual.

## Command editor

<kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>E</kbd> opens a text editor for long or multi-line commands, starting with what you typed. <kbd>Mod</kbd>+<kbd>Enter</kbd> runs it; <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>Enter</kbd> puts it on the prompt. Multi-line text is sent as one bracketed paste, so the shell does not run the lines one by one.

## AI in the terminal

With [Local AI](/guide/local-ai) turned on:

- <kbd>Ctrl</kbd>+<kbd>&#96;</kbd> opens the AI command bar: describe what you want, and the on-device model suggests one command with an explanation and a risk rating;
- or type `# what you want` at the prompt and press <kbd>Enter</kbd> (with [shell integration](#shell-integration), so that `#` lines elsewhere — in a heredoc or a REPL — are left alone);
- after a command fails, **Fix with AI** in its block menu suggests a corrected command from the command and its output.

The model gets the terminal's operating system, working directory and recent commands as context. Suggestions are put on the prompt (<kbd>Tab</kbd>) or run (<kbd>Mod</kbd>+<kbd>Enter</kbd>) only when you say so. AI agents connected over MCP can also [read your open terminals and type into them](/guide/ai-agents#terminals) with your approval.

## Shell integration

Command blocks, exit codes, the working directory for new tabs and the best suggestions need **shell integration**: a few lines of shell script that mark where prompts, commands and output start (OSC 133 / 633) and report the working directory (OSC 7). It is on by default (**Settings → Terminal → Command blocks**).

- **Local shells** (bash, zsh, fish) load it automatically, without changing your dotfiles.
- **SSH sessions**: open the command palette and choose **Enable command blocks in this session**, or set **Shell integration over SSH** to **Automatic**. Vaultport then types a one-line loader that turns echo off, reads the script for bash or zsh and evaluates it in the running shell. Nothing on the server is changed and the next login starts clean. To have it permanently, source the [script for your shell](https://github.com/DavidNiessen/vaultport/tree/main/crates/term-intel/scripts) from your remote dotfile; it only activates when `VAULTPORT_SHELL_INTEGRATION` is set, which Vaultport sends with every SSH session.

## More

- **Search** the output with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> (<kbd>⌘</kbd>+<kbd>F</kbd> on macOS): results update as you type, show the match count, and can match case, whole words or a regular expression.
- **Tab activity**: a tab shows a spinner while a command runs in it, and a check, a cross or a bell when a command finished, failed or the bell rang while you were elsewhere. **Bell** in Settings → Terminal chooses between a flash, a short tone or nothing.
- **Zoom**: <kbd>Mod</kbd>+<kbd>=</kbd> / <kbd>Mod</kbd>+<kbd>-</kbd> / <kbd>Mod</kbd>+<kbd>0</kbd>; **clear** with <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>K</kbd>.
- **macOS line editing**: <kbd>⌘</kbd>+<kbd>←</kbd>/<kbd>→</kbd> jump to the start or end of the line, <kbd>⌥</kbd>+<kbd>←</kbd>/<kbd>→</kbd> move by word, <kbd>⌘</kbd>+<kbd>⌫</kbd> deletes to the start of the line and <kbd>⌥</kbd>+<kbd>⌫</kbd> deletes a word.
- **Themes, fonts, truecolor, OSC 52 clipboard, remote titles** and more are described under [Hosts & groups](/guide/hosts-and-groups#terminal-features); the full list of shortcuts is under [Keyboard shortcuts](/guide/keyboard-shortcuts).
