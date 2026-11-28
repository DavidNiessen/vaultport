---
title: Snippets & autocomplete
description: Save reusable commands with variables, organize them in packages, run them on one or many hosts, and use command history and deterministic (non-AI) completion.
---

# Snippets & autocomplete

## Snippets

A **snippet** is a saved command or multi-line script with a description, for example _Restart the web server_. Snippets live in the vault (and therefore sync), and can be grouped into **packages**.

Create one in **Snippets → New snippet**.

### Variables

Use placeholders for values that change between runs:

| Syntax             | Meaning                              |
| ------------------ | ------------------------------------ |
| `{{name}}`         | Asked for before the snippet runs    |
| `{{name:default}}` | Asked for, pre-filled with `default` |

Variable names start with a letter or underscore and contain letters, digits and underscores. The same variable may appear several times; you are asked once.

```bash
sudo systemctl restart {{service:nginx}}
journalctl -u {{service:nginx}} --since "{{since:10 min ago}}" --no-pager
```

### Running snippets

**From the Snippets screen**, **Run** asks for the variables and the **target hosts**. Each target gets a new terminal tab that logs in and runs the script. The snippet's run counter goes up, so your most-used commands are easy to spot.

**From a terminal**, open the side panel (<kbd>Mod</kbd>+<kbd>.</kbd>) on **Snippets**:

- **Run** types the snippet into the current tab and presses Enter (double-click does the same),
- **Paste** types it without pressing Enter, so you can review or edit it first,
- **All tabs** runs it in every connected terminal tab.

The command palette (<kbd>Mod</kbd>+<kbd>K</kbd>) can also run a snippet in the current tab. Snippets whose variables have no default must be run from the Snippets screen, where you can fill them in.

### Running on several hosts

Choose several target hosts in the **Run** dialog to execute the same snippet everywhere, each in its own tab, or use **All tabs** for sessions that are already open. For interactive work across many machines, see also [broadcast in workspaces](/guide/workspaces#broadcast).

### Startup snippets

Set a **startup snippet** on a host or group (SSH options) to run it automatically after login — for example `tmux new -A -s main`.

## Command history

When the remote shell reports its commands through [shell-integration sequences](/guide/hosts-and-groups#terminal-features), Vaultport recognizes each command you run and keeps a per-host, frecency-ranked **history** in the encrypted vault. Browse it in the terminal side panel under **History**: click a command to insert it, double-click to run it. To start with the commands you already ran on a server, turn on [Import remote shell history](/guide/hosts-and-groups#shell-integration-os-detection-and-history-import).

## Autocomplete: deterministic, no AI

Vaultport's completion engine is deliberately **not** AI. It is a small, offline, deterministic engine in the Rust core (`term-intel`) that ranks suggestions from:

- your command **history** for the host,
- your **snippets**,
- the **commands installed on the machine** (everything on `PATH`, the usual tool directories and shell builtins),
- **command specs** for about 300 tools (subcommands, flags and values of `git`, `docker`, `kubectl`, `systemctl`, `npm`, `cargo`, `gh`, `aws`, `terraform`, macOS tools and many more), matched by prefix or fuzzily,
- **values from the machine**: git branches, tags and remotes, `npm run` scripts, `make` and `just` targets, users and groups, SSH hosts, systemd units, network interfaces, signals and environment variables,
- **paths**: on this computer in local terminals, and on the server.

On a server, Vaultport reads directories and the few small files it needs (such as `.git/HEAD`, `package.json` or `/etc/passwd`) over an SFTP channel of the terminal's own SSH connection, so no second login is needed and nothing is run on the server. Without SFTP on the server, the rest still works.

It uses a quote-aware shell tokenizer, never runs anything, and works the same on every platform. You can switch it off in **Settings → Terminal → Suggestions**. See [The terminal](/guide/terminal#autocomplete) for inline suggestions, syntax highlighting and the rest of the terminal.

### Using it

Suggestions appear below the cursor as you type at the prompt:

| Key                              | Action                                          |
| -------------------------------- | ----------------------------------------------- |
| <kbd>↑</kbd> / <kbd>↓</kbd>      | Choose a suggestion                             |
| <kbd>Tab</kbd>                   | Insert it                                       |
| <kbd>→</kbd> or <kbd>End</kbd>   | Accept the grey inline suggestion from history  |
| <kbd>Ctrl</kbd>+<kbd>Space</kbd> | Show suggestions now                            |
| <kbd>Esc</kbd>                   | Close the list                                  |
| <kbd>Enter</kbd>                 | Close the list and run the line as you typed it |

While the list is closed, <kbd>Tab</kbd> and the arrow keys go to the shell as usual. Inserting a directory keeps the list open for the next level.

Suggestions work best with [shell integration](/guide/terminal#shell-integration) (on by default for local shells): Vaultport then knows exactly where your input starts, which directory you are in, and when a command is running. Without it, the prompt is recognized by its usual ending (`$`, `#`, `%`, `>` or `❯`), and paths only complete from absolute or `~/` paths. Nothing is suggested while a command runs, in full-screen programs such as `vim` or `htop`, or while a workspace broadcasts input to several terminals.

## Risk hints

Generated commands (see [Local AI](/guide/local-ai)) are rated by a deterministic risk checker that recognizes destructive patterns — recursive deletes, disk writes and formatting, permission changes, firewall flushes, SQL drops, force pushes, `curl … | sh`, fork bombs and more — including inside `sudo`, `sh -c '…'` and `ssh host '…'`. The rating is a hint for you, not a security boundary.
