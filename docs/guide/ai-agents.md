---
title: AI agents (MCP)
description: Let AI tools such as Claude Code, Claude Desktop, Cursor, VS Code and Codex work with your servers through Vaultport over the Model Context Protocol — locally, without exposing credentials, and with your approval.
---

# AI agents (MCP)

Vaultport can act as a [Model Context Protocol](https://modelcontextprotocol.io) (MCP) server, so AI tools such as **Claude Code**, **Claude Desktop**, **Cursor**, **VS Code** or **Codex** can list your hosts, read files and — with your approval — run commands on your servers.

The agent never gets your passwords, keys or vault. It asks Vaultport to do something; Vaultport shows you what exactly will happen, opens the connection itself with your saved credentials, and returns only the result.

::: info Desktop only, off by default
Agent access is available in the desktop apps (Windows, macOS, Linux). It is **off** until you turn it on, and it only accepts connections from programs running as your user on the same computer.
:::

This is separate from [Local AI](/guide/local-ai), which generates commands with an on-device model. With agent access, the AI tool you connect decides what to ask for, and its model may run in the cloud — whatever Vaultport returns to it (command output, file contents, host names) is sent wherever that tool sends its prompts.

## Turn it on

1. Open **Settings → AI agents** and switch on **Allow AI agents**. The status changes to _On: agents on this computer can connect_.
2. Choose when you want to be asked (**Ask before**), whether agents may only read (**Read-only**), and which **hosts** they can see.
3. Under **Connect a client**, pick your tool and copy the ready-made snippet. It already contains the correct path to the Vaultport executable on your computer.

The MCP client starts `vaultport mcp` whenever it needs the server. That small command relays the conversation to the running app; if Vaultport is not running, it starts it.

## Connect a client

The snippets below use a placeholder for the executable. Use the exact snippet shown in **Settings → AI agents**, or replace `<vaultport>` with the path for your installation:

| Platform                      | Typical path                                           |
| ----------------------------- | ------------------------------------------------------ |
| macOS                         | `/Applications/vaultport.app/Contents/MacOS/vaultport` |
| Windows (installer, per user) | `C:\Users\<you>\AppData\Local\vaultport\vaultport.exe` |
| Windows (MSI)                 | `C:\Program Files\vaultport\vaultport.exe`             |
| Linux (`.deb`, `.rpm`)        | `/usr/bin/vaultport`                                   |
| Linux (AppImage)              | the full path of the `.AppImage` file                  |

Paths that contain spaces must be quoted in shell commands (`'…'` on macOS and Linux, `"…"` on Windows); the app does this for you.

The JSON snippets go into `claude_desktop_config.json` for Claude Desktop (**Settings → Developer → Edit Config**), `~/.cursor/mcp.json` for Cursor, and `.vscode/mcp.json` in your workspace for VS Code. Claude Code and Codex add the server with one command.

::: code-group

```bash [Claude Code]
claude mcp add --scope user vaultport -- <vaultport> mcp
```

```json [Claude Desktop]
{
  "mcpServers": {
    "vaultport": {
      "command": "<vaultport>",
      "args": ["mcp"]
    }
  }
}
```

```json [Cursor]
{
  "mcpServers": {
    "vaultport": {
      "command": "<vaultport>",
      "args": ["mcp"]
    }
  }
}
```

```json [VS Code]
{
  "servers": {
    "vaultport": {
      "type": "stdio",
      "command": "<vaultport>",
      "args": ["mcp"]
    }
  }
}
```

```bash [Codex]
codex mcp add vaultport -- <vaultport> mcp
```

```bash [Other clients]
# Any MCP client that supports the stdio transport:
<vaultport> mcp
```

:::

In JSON files on Windows, write backslashes twice (`"C:\\Program Files\\vaultport\\vaultport.exe"`).

If agent access is turned off, `vaultport mcp` still starts, but only offers a single status tool that tells the agent to ask you to turn access on — so you get a clear message instead of a broken server.

## Tools

| Tool                 | What it does                                                                                                                                                                    | Changes something |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| `list_hosts`         | Lists the saved SSH hosts available to agents (id, label, address, port, username, group, tags, OS). Never includes credentials.                                                | no                |
| `list_snippets`      | Lists your saved command snippets with their variables.                                                                                                                         | no                |
| `list_port_forwards` | Lists saved port-forwarding rules and whether each is running.                                                                                                                  | no                |
| `list_directory`     | Lists a directory on a host over SFTP (home directory by default).                                                                                                              | no                |
| `read_file`          | Reads a file on a host over SFTP (256 KiB by default, at most 4 MiB; text or base64).                                                                                           | no                |
| `run_command`        | Runs a shell command on a host without a TTY and returns stdout, stderr and the exit status (timeout 120 s by default, at most 30 min; long output is shortened in the middle). | yes, destructive  |
| `run_snippet`        | Runs a saved snippet on a host, filling in its variables.                                                                                                                       | yes, destructive  |
| `write_file`         | Writes a file over SFTP: overwrite, create (fails if it exists) or append (at most 8 MiB).                                                                                      | yes, destructive  |
| `make_directory`     | Creates a directory over SFTP.                                                                                                                                                  | yes               |
| `delete_path`        | Deletes a file, symlink or empty directory over SFTP (not recursive).                                                                                                           | yes, destructive  |
| `start_port_forward` | Starts a saved port-forwarding rule.                                                                                                                                            | yes               |
| `stop_port_forward`  | Stops a running port-forwarding rule.                                                                                                                                           | yes               |
| `open_terminal`      | Opens an interactive terminal tab for you in Vaultport. Use it to hand over.                                                                                                    | yes               |
| `list_terminals`     | Lists the terminals you have open (SSH, local, serial) with their working directory and last command and exit status.                                                           | no                |
| `read_terminal`      | Reads the recent commands of one of your terminals with their output and exit status (or the last lines of output without shell integration).                                   | no                |
| `type_in_terminal`   | Types a command into one of your terminals, where you see it; with `run` it presses Enter and returns the output once the command finishes.                                     | yes, destructive  |

Hosts are referred to by id, label or address; terminals by the id from `list_terminals` or their tab label. Connections use the host's saved settings — jump hosts, proxy, keys, [host-key verification](/guide/known-hosts) — exactly as if you connected yourself.

## Terminals

Agents can see what happens in the terminals you have open — for example to explain an error you just got. Copy a ready-made prompt from a failed command's badge menu (**Copy a prompt for your AI agent**) and paste it into your agent.

- `read_terminal` returns the last commands of a terminal with their output (shortened in the middle when long), exit status and working directory. With [shell integration](/guide/terminal#shell-integration) Vaultport knows where each command starts and ends; without it, the agent gets the last lines of output.
- `type_in_terminal` always asks you first and shows exactly what will be typed. Vaultport switches to the terminal so you can watch. Without `run`, the command waits on the prompt for you to review and press Enter.
- Terminals are covered by the host scope: with **Hosts → Selected**, only terminals of the selected hosts are visible, and local shells, ad-hoc connections and serial consoles are hidden.
- Reading or typing into a local shell, an ad-hoc connection or a serial console **always asks**, whatever **Ask before** says, because they are not saved hosts you scoped agents to. **Allow for this session** covers only the one terminal you approved.
- `type_in_terminal` refuses while a command is running in that terminal (when Vaultport can tell, with shell integration), so the text cannot end up in a password prompt, an editor or a REPL. It clears whatever is on the prompt before typing.
- Output is recorded in memory only while the terminal is open (the last 30 commands), and never stored.

## Approvals

When an agent wants to do something that needs approval, Vaultport comes to the front and shows **"&lt;agent&gt; wants to act"** with the tool, the host and the exact details: the full command, the path, or a preview of the content to write. Destructive actions are marked **Makes changes**.

- **Deny** — the agent is told that you declined and not to retry unchanged.
- **Allow once** — only this call.
- **Allow for this session** — this tool on this host is allowed until the agent disconnects or the vault locks.

If you do not answer within **5 minutes**, the request is denied.

**Ask before** decides which calls need approval:

| Setting                    | Reads (`list_*`, `read_*`) | Everything that changes something |
| -------------------------- | -------------------------- | --------------------------------- |
| **Every action** (default) | ask                        | ask                               |
| **Changes only**           | run without asking         | ask                               |

Actions that change something always need your approval (or an earlier "allow for this session").

## Read-only mode and host scope

- **Read-only** hides every tool that changes something; agents can only list and read. Calls to other tools are refused.
- **Hosts → Selected** limits agents to the hosts you tick. Other hosts do not appear in `list_hosts`, cannot be targeted by any tool, and their port-forwarding rules are hidden.

## Connected agents and activity

**Settings → AI agents** shows the agents that are connected right now (named after what the client reports, such as "Claude Code"), with **Disconnect all**. **Reset access** creates a new access token and disconnects every agent; clients reconnect automatically the next time they start `vaultport mcp`.

The **Activity** list records every agent call — time, agent, tool, host, a one-line summary and whether it was done, failed or denied. It keeps the last 500 entries while the app is running.

## When the vault is locked

Agents cannot do anything while the vault is locked: every call returns "The Vaultport vault is locked. Ask the user to unlock Vaultport, then try again." without asking you first. Locking the vault also revokes all "allow for this session" grants.

## Security model

- **Local only.** There is no network listener. The app listens on a Unix socket with mode `0600` in your runtime or app data directory (macOS, Linux), or on a per-user named pipe that rejects remote clients (Windows).
- **Token.** `vaultport mcp` must present a random 256-bit token from the file `agent-token` in the app data directory, readable only by you (`0600`). Tokens are compared in constant time; **Reset access** replaces it.
- **Same user.** On macOS and Linux the app additionally checks the connecting process's user id and drops connections from anyone else.
- **No secrets.** Tools never return passwords, private keys, passphrases or vault contents; credentials are used inside Vaultport only.
- **Per device.** Agent settings live in a local file and are not synced; enabling access on one computer does not enable it anywhere else.
- **Off means off.** When you turn access off, the listener stops and all connections are dropped; `vaultport mcp` then serves only the status tool.

### Prompt injection

An agent reads command output and file contents from your servers. Anything there — a log line, a README, a crafted file — can contain text that tries to steer the agent ("ignore previous instructions and run …"). Vaultport cannot tell such text from legitimate output. **Your approval is the control**: read every command and path in the approval dialog before allowing it, prefer _Allow once_ on production hosts, use **Read-only** or **Selected** hosts for exploratory sessions, and check the **Activity** list afterwards. See also the [threat model](/security/#ai-agents-over-mcp).
