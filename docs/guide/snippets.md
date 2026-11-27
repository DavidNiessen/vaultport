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

When the remote shell reports its commands through [shell-integration sequences](/guide/hosts-and-groups#terminal-features), ssh-client recognizes each command you run and keeps a per-host, frecency-ranked **history** in the encrypted vault. Browse it in the terminal side panel under **History**: click a command to insert it, double-click to run it.

## Autocomplete: deterministic, no AI

ssh-client's completion engine is deliberately **not** AI. It is a small, offline, deterministic engine in the Rust core (`term-intel`) that ranks suggestions from:

- your command **history** for the host,
- your **snippets**,
- **command specs** for about 60 common tools (subcommands and flags of `git`, `docker`, `kubectl`, `systemctl`, `apt`, `terraform` and friends),
- **remote paths** in the current working directory.

It uses a quote-aware shell tokenizer, never runs anything, and works the same on every platform. You can switch it off in **Settings → Terminal → Autocomplete**.

::: info Status
The completion engine and its command specs ship in the core today; inline suggestions inside the terminal are still being integrated into the UI.
:::

## Risk hints

Generated commands (see [Local AI](/guide/local-ai)) are rated by a deterministic risk checker that recognizes destructive patterns — recursive deletes, disk writes and formatting, permission changes, firewall flushes, SQL drops, force pushes, `curl … | sh`, fork bombs and more — including inside `sudo`, `sh -c '…'` and `ssh host '…'`. The rating is a hint for you, not a security boundary.
