---
title: Hosts & groups
description: Add, organize, search and connect to hosts; tags, favorites, per-host terminal appearance, SSH options and the local terminal.
---

# Hosts & groups

The **Hosts** section of the vault is where you manage the machines you connect to.

## Adding a host

Choose **New host** and fill in what you need; everything is saved automatically.

**General**

- **Label** (shown in lists and tabs), **address** and **port**.
- **Parent group**, **tags** and **notes**. Mark frequently used hosts as **favorites**.

**Credentials**

- **Use an identity** from the keychain, or **Custom credentials…** with a username, a password (stored encrypted) and keys. See [Identities & keychain](/concepts/identities-and-keychain).

**SSH options**

| Option                    | What it does                                                                                                       |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Jump hosts                | Connect through other saved hosts, in order (ProxyJump). See [Jump hosts & proxies](/guide/jump-hosts-and-proxies) |
| Proxy                     | HTTP CONNECT or SOCKS5 proxy for the first hop                                                                     |
| Agent forwarding          | Lets the server use your keys for onward connections (with optional per-use confirmation)                          |
| Environment variables     | Sent on connect; the server must allow them with `AcceptEnv`                                                       |
| Startup snippet           | A [snippet](/guide/snippets) run right after login                                                                 |
| Keepalive interval        | Seconds between keepalive messages (default 30)                                                                    |
| Compression               | Enables `zlib@openssh.com` / `zlib`                                                                                |
| Post-quantum key exchange | Prefers `mlkem768x25519-sha256`, falling back automatically (on by default)                                        |
| Allow legacy algorithms   | Adds SHA-1, `ssh-rsa` and CBC ciphers for very old servers — always after the secure choices                       |
| Strict host key checking  | Refuse unknown hosts instead of asking. See [Known hosts](/guide/known-hosts)                                      |

**Other protocols** — enable [Mosh](/guide/mosh) or [Telnet](/guide/telnet) for the host.

**Terminal** — theme, font family and size, backspace key, terminal type, encoding and scrollback can be set per host or per group.

Any of these can instead be set on a group and [inherited](/concepts/groups-and-inheritance).

## Groups

Create a group with **New group**, nest it under another group, and move hosts into it by choosing their parent group. Opening a group shows its hosts and subgroups, and its own settings editor. Deleting a group never deletes hosts: its hosts and subgroups move up to the parent group.

## Finding hosts

- The search field filters by label, address and tags (every word must match); **Filter by tags** narrows the list further. Favorites are listed first.
- Switch between **grid view** and **list view**.
- <kbd>Mod</kbd>+<kbd>J</kbd> jumps to any host or open tab from anywhere, and <kbd>Mod</kbd>+<kbd>K</kbd> opens the command palette with hosts, tabs, snippets and actions.
- Type a connection string such as `ssh -p 2222 alice@example.com` into the search field and press <kbd>Enter</kbd> to [quick-connect](/guide/quick-start#_2-add-a-host) without saving a host.

## Connecting

Double-click a host, press <kbd>Enter</kbd> on it, or use **Connect**. Select several hosts and choose **Connect all**, or create a [workspace](/guide/workspaces) from the selection. Use **Open in SFTP** to browse its files instead.

While connected, the tab's status dot shows the state: connecting, connected (with an indicator for unread output in background tabs), error or disconnected. If the network drops, ssh-client reconnects automatically (Settings → Terminal → _Reconnect automatically after network loss_), and **Reconnect** is always one click away.

## The local terminal

On desktop, <kbd>Mod</kbd>+<kbd>L</kbd> opens your local shell in a new tab. It starts your login shell (`$SHELL`, or Windows PowerShell on Windows). To use another program, enter it under **Settings → Terminal → Local shell**, for example `/opt/homebrew/bin/fish` or `pwsh.exe`; leave the field empty for the system default.

With **Shell integration** on, local bash, zsh and fish shells load ssh-client's integration script automatically, without changes to your dotfiles. Your usual startup files are still read.

## Terminal features

- **Search** the output with <kbd>Mod</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd>.
- **Themes**: the original Midnight, Daylight, Ember (production) and Glacier, plus Dracula, Nord, Gruvbox, Catppuccin, Tokyo Night, Solarized, One Dark and more; per host, per group, or globally.
- **Keyword highlighting** for errors, warnings and IP addresses (optional).
- **OSC 52** clipboard writes from tmux or vim: _Ask_ (default), _Allow_ or _Deny_.
- **Remote titles**: let programs set the tab title.
- Copy on select, right-click to paste, Option as Meta key (macOS), cursor style and blink, scrollback size (10 000 lines by default).
- **Shell integration**: if the remote shell emits the common shell-integration escape sequences (OSC 133 or OSC 633 prompt and command marks, OSC 7 working directory), ssh-client records each command in the host's [history](/guide/snippets#command-history). Password prompts are detected so that what you type there is never recorded.

## Shell integration, OS detection and history import

These switches are under **Settings → Terminal**.

- **Shell integration** (off by default) loads ssh-client's integration script into [local shells](#the-local-terminal). For SSH sessions it sends the variable `SSH_CLIENT_SHELL_INTEGRATION=1` with the shell request. The scripts for bash, zsh and fish are in [`crates/term-intel/scripts`](https://github.com/davidniessen/ssh-client/tree/main/crates/term-intel/scripts). They do nothing unless that variable is set, so you can source one from your remote dotfile (for example `~/.bashrc`). The server must accept the variable (`AcceptEnv SSH_CLIENT_SHELL_INTEGRATION` in `sshd_config`); otherwise export it yourself before sourcing the script.
- **Detect remote operating system** (on by default) checks `/etc/os-release` or `uname` after you log in to a saved host whose operating system is not set, and fills it in so the host shows the right icon. An operating system you picked yourself is never changed.
- **Import remote shell history** (off by default) reads the end of `~/.bash_history`, `~/.zsh_history` and `~/.local/share/fish/fish_history` once per saved host and adds the commands to the host's [history](/guide/snippets#command-history).

Detection and import each open a short-lived extra SSH channel in the background after the terminal is up. Turn them off for servers that allow only one session per connection (`MaxSessions 1`). They are not used for Mosh sessions.
