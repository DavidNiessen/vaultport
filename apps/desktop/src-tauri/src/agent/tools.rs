// SPDX-License-Identifier: GPL-3.0-or-later
//! The MCP tools offered to AI agents.

use core_model::{Group, Host, PortForwardRule, Snippet};
use mcp_server::{CallContext, Tool, ToolAnnotations, ToolResult};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{
    AgentApprovalRequest, AgentState, ApprovalPolicy, AuditEntry, AuditOutcome,
    OPEN_TERMINAL_EVENT, now_ms,
};
use crate::commands::files::{FilesState, FsSource};
use crate::commands::forwarding;
use crate::error::{CmdError, ErrorCode};
use crate::state::AppState;

/// Usage notes sent to the model in `initialize`.
pub const INSTRUCTIONS: &str = "Vaultport gives you access to the servers saved in the user's \
Vaultport vault. Start with list_hosts and refer to hosts by their id or label. Credentials \
never leave Vaultport: connections are opened by the app with the user's saved keys. Actions \
may require the user's approval in the app, so calls can take a while; if a call is denied, do \
not retry it unchanged — ask the user. Commands run non-interactively (no TTY): avoid pagers and \
prompts (use flags like -y, --no-pager), and prefer read-only inspection before changing \
anything. Use open_terminal when the user should take over interactively. list_terminals and \
read_terminal show what happens in the terminals the user has open (e.g. to explain an error \
they just got); type_in_terminal types into one of them where the user can watch.";

const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_TIMEOUT_SECS: u64 = 1800;
const MAX_EXEC_OUTPUT: usize = 8 * 1024 * 1024;
/// Characters of stdout/stderr returned to the model (head and tail are kept).
const MAX_RETURNED_OUTPUT: usize = 60_000;
const DEFAULT_READ_BYTES: u64 = 256 * 1024;
const MAX_READ_BYTES: u64 = 4 * 1024 * 1024;
const MAX_WRITE_BYTES: usize = 8 * 1024 * 1024;
const MAX_LIST_ENTRIES: usize = 2000;

struct Def {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    /// Changes something (always needs approval unless granted for the session).
    changes: bool,
    destructive: bool,
    schema: fn() -> Value,
}

const HOST: &str = "Host id, label or address (see list_hosts).";
const TERMINAL: &str = "Terminal id or label (see list_terminals).";
/// Output returned by the terminal tools, in characters.
const MAX_TERMINAL_OUTPUT: usize = 40_000;

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

const DEFS: &[Def] = &[
    Def {
        name: "list_hosts",
        title: "List hosts",
        description: "Lists the SSH hosts the user saved in Vaultport (id, label, address, port, \
                      username, group, tags, OS). Never includes credentials.",
        changes: false,
        destructive: false,
        schema: || {
            obj(
                json!({ "query": { "type": "string", "description": "Filter by label, address, group or tag." } }),
                &[],
            )
        },
    },
    Def {
        name: "list_snippets",
        title: "List snippets",
        description: "Lists the user's saved command snippets with their {{variables}}.",
        changes: false,
        destructive: false,
        schema: || {
            obj(
                json!({ "query": { "type": "string", "description": "Filter by label, script or tag." } }),
                &[],
            )
        },
    },
    Def {
        name: "list_port_forwards",
        title: "List port forwards",
        description: "Lists saved port-forwarding rules and whether each is running.",
        changes: false,
        destructive: false,
        schema: || obj(json!({}), &[]),
    },
    Def {
        name: "list_directory",
        title: "List directory",
        description: "Lists a directory on a host over SFTP (defaults to the home directory).",
        changes: false,
        destructive: false,
        schema: || {
            obj(
                json!({
                    "host": { "type": "string", "description": HOST },
                    "path": { "type": "string", "description": "Absolute or home-relative path." }
                }),
                &["host"],
            )
        },
    },
    Def {
        name: "read_file",
        title: "Read file",
        description: "Reads a file on a host over SFTP. Text is returned as is; use encoding \
                      'base64' for binary files.",
        changes: false,
        destructive: false,
        schema: || {
            obj(
                json!({
                    "host": { "type": "string", "description": HOST },
                    "path": { "type": "string" },
                    "max_bytes": { "type": "integer", "minimum": 1, "maximum": MAX_READ_BYTES, "default": DEFAULT_READ_BYTES },
                    "encoding": { "type": "string", "enum": ["text", "base64"], "default": "text" }
                }),
                &["host", "path"],
            )
        },
    },
    Def {
        name: "run_command",
        title: "Run command",
        description: "Runs a shell command on a host (non-interactive, no TTY) and returns stdout, \
                      stderr and the exit status. Long output is shortened in the middle.",
        changes: true,
        destructive: true,
        schema: || {
            obj(
                json!({
                    "host": { "type": "string", "description": HOST },
                    "command": { "type": "string" },
                    "stdin": { "type": "string", "description": "Text written to the command's standard input." },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_SECS, "default": DEFAULT_TIMEOUT_SECS }
                }),
                &["host", "command"],
            )
        },
    },
    Def {
        name: "run_snippet",
        title: "Run snippet",
        description: "Runs a saved snippet on a host, filling its {{variables}} (defaults apply \
                      when a value is omitted).",
        changes: true,
        destructive: true,
        schema: || {
            obj(
                json!({
                    "host": { "type": "string", "description": HOST },
                    "snippet": { "type": "string", "description": "Snippet id or label (see list_snippets)." },
                    "variables": { "type": "object", "additionalProperties": { "type": "string" } },
                    "timeout_seconds": { "type": "integer", "minimum": 1, "maximum": MAX_TIMEOUT_SECS, "default": DEFAULT_TIMEOUT_SECS }
                }),
                &["host", "snippet"],
            )
        },
    },
    Def {
        name: "write_file",
        title: "Write file",
        description: "Writes a file on a host over SFTP. mode: 'overwrite' (default), 'create' \
                      (fail if it exists) or 'append'.",
        changes: true,
        destructive: true,
        schema: || {
            obj(
                json!({
                    "host": { "type": "string", "description": HOST },
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "encoding": { "type": "string", "enum": ["text", "base64"], "default": "text" },
                    "mode": { "type": "string", "enum": ["overwrite", "create", "append"], "default": "overwrite" }
                }),
                &["host", "path", "content"],
            )
        },
    },
    Def {
        name: "make_directory",
        title: "Make directory",
        description: "Creates a directory on a host over SFTP.",
        changes: true,
        destructive: false,
        schema: || {
            obj(
                json!({ "host": { "type": "string", "description": HOST }, "path": { "type": "string" } }),
                &["host", "path"],
            )
        },
    },
    Def {
        name: "delete_path",
        title: "Delete file or empty directory",
        description: "Deletes a file, symlink or empty directory on a host over SFTP (not recursive).",
        changes: true,
        destructive: true,
        schema: || {
            obj(
                json!({ "host": { "type": "string", "description": HOST }, "path": { "type": "string" } }),
                &["host", "path"],
            )
        },
    },
    Def {
        name: "start_port_forward",
        title: "Start port forward",
        description: "Starts a saved port-forwarding rule.",
        changes: true,
        destructive: false,
        schema: || {
            obj(
                json!({ "rule": { "type": "string", "description": "Rule id or label (see list_port_forwards)." } }),
                &["rule"],
            )
        },
    },
    Def {
        name: "stop_port_forward",
        title: "Stop port forward",
        description: "Stops a running port-forwarding rule.",
        changes: true,
        destructive: false,
        schema: || {
            obj(
                json!({ "rule": { "type": "string", "description": "Rule id or label." } }),
                &["rule"],
            )
        },
    },
    Def {
        name: "list_terminals",
        title: "List open terminals",
        description: "Lists the terminal tabs the user has open in Vaultport (SSH, local shell, \
                      serial) with their working directory and the last command and its exit \
                      status. Use read_terminal to see their output.",
        changes: false,
        destructive: false,
        schema: || obj(json!({}), &[]),
    },
    Def {
        name: "read_terminal",
        title: "Read terminal output",
        description: "Reads what happened in one of the user's open terminals: the most recent \
                      commands with their output and exit status (when the shell has Vaultport's \
                      shell integration), or else the last lines of output. Use it to see what \
                      the user is looking at, e.g. to explain an error.",
        changes: false,
        destructive: false,
        schema: || {
            obj(
                json!({
                    "terminal": { "type": "string", "description": TERMINAL },
                    "commands": { "type": "integer", "minimum": 1, "maximum": 20, "default": 5, "description": "How many recent commands to return." },
                    "lines": { "type": "integer", "minimum": 1, "maximum": 1000, "default": 120, "description": "Lines of plain output to return when commands are not tracked." }
                }),
                &["terminal"],
            )
        },
    },
    Def {
        name: "type_in_terminal",
        title: "Type in terminal",
        description: "Types a command into one of the user's open terminals, where the user sees \
                      it. With run=true it presses Enter and waits for the command to finish \
                      (when the shell has shell integration) and returns its output; otherwise \
                      the user can review and run it. Interactive programs keep running in the \
                      user's terminal.",
        changes: true,
        destructive: true,
        schema: || {
            obj(
                json!({
                    "terminal": { "type": "string", "description": TERMINAL },
                    "text": { "type": "string", "description": "One command line (no newlines)." },
                    "run": { "type": "boolean", "default": false, "description": "Press Enter after typing." },
                    "wait_seconds": { "type": "integer", "minimum": 0, "maximum": MAX_TIMEOUT_SECS, "default": 30, "description": "With run: how long to wait for the command to finish." }
                }),
                &["terminal", "text"],
            )
        },
    },
    Def {
        name: "open_terminal",
        title: "Open terminal",
        description: "Opens an interactive terminal tab for the user in Vaultport. You cannot \
                      read or type into it; use it to hand over to the user.",
        changes: true,
        destructive: false,
        schema: || {
            obj(
                json!({ "host": { "type": "string", "description": HOST } }),
                &["host"],
            )
        },
    },
];

/// The MCP handler for one agent connection.
pub struct AgentHandler {
    app: AppHandle,
    state: Arc<AgentState>,
}

impl std::fmt::Debug for AgentHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentHandler").finish_non_exhaustive()
    }
}

impl AgentHandler {
    pub fn new(app: AppHandle, state: Arc<AgentState>) -> Self {
        Self { app, state }
    }
}

/// A tool call checked and ready to run.
enum Plan {
    ListHosts(Option<String>),
    ListSnippets(Option<String>),
    ListForwards,
    ListDir {
        host: Host,
        path: Option<String>,
    },
    Read {
        host: Host,
        path: String,
        max: u64,
        base64: bool,
    },
    Exec {
        host: Host,
        command: String,
        stdin: Option<String>,
        timeout: Duration,
    },
    Write {
        host: Host,
        path: String,
        data: Vec<u8>,
        mode: WriteMode,
    },
    Mkdir {
        host: Host,
        path: String,
    },
    Delete {
        host: Host,
        path: String,
    },
    StartForward(PortForwardRule),
    StopForward(PortForwardRule),
    OpenTerminal(Host),
    ListTerminals,
    ReadTerminal {
        terminal: Terminal,
        commands: usize,
        lines: usize,
    },
    TypeInTerminal {
        terminal: Terminal,
        text: String,
        run: bool,
        wait: Duration,
    },
}

/// An open terminal session agents may see.
#[derive(Clone)]
struct Terminal {
    id: String,
    info: crate::sessions::SessionInfo,
    blocks: crate::sessions::Blocks,
}

/// Event asking the UI to show the tab of a session (payload: the session id).
pub const SHOW_SESSION_EVENT: &str = "agent://show-session";

#[derive(Clone, Copy, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum WriteMode {
    #[default]
    Overwrite,
    Create,
    Append,
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum Encoding {
    #[default]
    Text,
    Base64,
}

fn cmd_err(e: &CmdError) -> String {
    match e.code {
        ErrorCode::Locked | ErrorCode::NoVault => {
            "The Vaultport vault is locked. Ask the user to unlock Vaultport, then try again."
                .to_owned()
        }
        _ => e.message.clone(),
    }
}

fn args<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, String> {
    serde_json::from_value(v).map_err(|e| format!("invalid arguments: {e}"))
}

fn label(h: &Host) -> String {
    if h.label.is_empty() {
        h.address.clone()
    } else {
        h.label.clone()
    }
}

fn timeout(secs: Option<u64>) -> Duration {
    Duration::from_secs(
        secs.unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS),
    )
}

/// Keeps the start and the end of long output.
fn shorten(text: &str, max: usize) -> (String, bool) {
    let count = text.chars().count();
    if count <= max {
        return (text.to_owned(), false);
    }
    let head: String = text.chars().take(max / 3).collect();
    let tail: String = text.chars().skip(count - (max - max / 3)).collect();
    let omitted = count - head.chars().count() - tail.chars().count();
    (
        format!("{head}\n… [{omitted} characters omitted] …\n{tail}"),
        true,
    )
}

fn matches(query: Option<&str>, fields: &[&str]) -> bool {
    let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) else {
        return true;
    };
    let q = q.to_lowercase();
    fields.iter().any(|f| f.to_lowercase().contains(&q))
}

fn group_path(groups: &[Group], id: Option<core_model::GroupId>) -> Option<String> {
    let mut names = Vec::new();
    let mut cur = id;
    // Bounded walk: the model forbids cycles, but never loop forever.
    for _ in 0..64 {
        let Some(g) = cur.and_then(|id| groups.iter().find(|g| g.id == id)) else {
            break;
        };
        names.push(g.label.clone());
        cur = g.parent;
    }
    if names.is_empty() {
        return None;
    }
    names.reverse();
    Some(names.join(" / "))
}

impl AgentHandler {
    fn hosts_in_scope(&self) -> Result<(Vec<Host>, Vec<Group>), String> {
        let settings = self.state.settings();
        let (hosts, groups) = self
            .app
            .state::<AppState>()
            .with_vault(|v| Ok((v.list::<Host>()?, v.list::<Group>()?)))
            .map_err(|e| cmd_err(&e))?;
        let hosts = hosts
            .into_iter()
            .filter(|h| settings.all_hosts || settings.hosts.contains(&h.id.to_string()))
            .collect();
        Ok((hosts, groups))
    }

    fn terminals_in_scope(&self) -> Vec<Terminal> {
        let settings = self.state.settings();
        self.app
            .state::<crate::sessions::SessionManager>()
            .list()
            .into_iter()
            .filter(|(_, info, _)| match &info.host_id {
                Some(h) => settings.all_hosts || settings.hosts.contains(h),
                // Local shells and ad-hoc connections are not saved hosts the user could pick.
                None => settings.all_hosts,
            })
            .map(|(id, info, blocks)| Terminal { id, info, blocks })
            .collect()
    }

    fn terminal(&self, needle: &str) -> Result<Terminal, String> {
        let all = self.terminals_in_scope();
        let n = needle.trim();
        if let Some(t) = all.iter().find(|t| t.id == n) {
            return Ok(t.clone());
        }
        let lower = n.to_lowercase();
        let found: Vec<&Terminal> = all
            .iter()
            .filter(|t| t.info.label.to_lowercase() == lower)
            .collect();
        match found.as_slice() {
            [t] => Ok((*t).clone()),
            [] => Err(format!(
                "no open terminal '{n}' is available to agents (see list_terminals)"
            )),
            _ => Err(format!(
                "'{n}' matches several terminals; use an id from list_terminals"
            )),
        }
    }

    fn host(&self, needle: &str) -> Result<Host, String> {
        let (hosts, _) = self.hosts_in_scope()?;
        let n = needle.trim();
        if let Some(h) = hosts.iter().find(|h| h.id.to_string() == n) {
            return Ok(h.clone());
        }
        let lower = n.to_lowercase();
        for pick in [
            |h: &Host, n: &str| h.label.to_lowercase() == n,
            |h: &Host, n: &str| h.address.to_lowercase() == n,
        ] {
            let found: Vec<&Host> = hosts.iter().filter(|h| pick(h, &lower)).collect();
            match found.as_slice() {
                [h] => return Ok((*h).clone()),
                [] => {}
                many => {
                    let ids: Vec<String> = many
                        .iter()
                        .map(|h| format!("{} ({})", h.id, h.address))
                        .collect();
                    return Err(format!(
                        "'{n}' matches several hosts; use an id: {}",
                        ids.join(", ")
                    ));
                }
            }
        }
        Err(format!(
            "no host '{n}' is available to agents (see list_hosts)"
        ))
    }

    fn rule(&self, needle: &str) -> Result<PortForwardRule, String> {
        let rules = self
            .app
            .state::<AppState>()
            .with_vault(|v| Ok(v.list::<PortForwardRule>()?))
            .map_err(|e| cmd_err(&e))?;
        let n = needle.trim();
        let lower = n.to_lowercase();
        let rule = rules
            .iter()
            .find(|r| r.id.to_string() == n)
            .or_else(|| rules.iter().find(|r| r.label.to_lowercase() == lower))
            .cloned()
            .ok_or_else(|| format!("no port-forwarding rule '{n}'"))?;
        // The rule's host must be in scope too.
        self.host(&rule.host.to_string())?;
        Ok(rule)
    }

    fn plan(&self, name: &str, a: Value) -> Result<Plan, String> {
        #[derive(Deserialize)]
        struct Query {
            query: Option<String>,
        }
        #[derive(Deserialize)]
        struct HostPath {
            host: String,
            path: String,
        }
        Ok(match name {
            "list_hosts" => Plan::ListHosts(args::<Query>(a)?.query),
            "list_snippets" => Plan::ListSnippets(args::<Query>(a)?.query),
            "list_port_forwards" => Plan::ListForwards,
            "list_directory" => {
                #[derive(Deserialize)]
                struct A {
                    host: String,
                    path: Option<String>,
                }
                let a: A = args(a)?;
                Plan::ListDir {
                    host: self.host(&a.host)?,
                    path: a.path.filter(|p| !p.trim().is_empty()),
                }
            }
            "read_file" => {
                #[derive(Deserialize)]
                struct A {
                    host: String,
                    path: String,
                    max_bytes: Option<u64>,
                    #[serde(default)]
                    encoding: Encoding,
                }
                let a: A = args(a)?;
                Plan::Read {
                    host: self.host(&a.host)?,
                    path: a.path,
                    max: a
                        .max_bytes
                        .unwrap_or(DEFAULT_READ_BYTES)
                        .clamp(1, MAX_READ_BYTES),
                    base64: matches!(a.encoding, Encoding::Base64),
                }
            }
            "run_command" => {
                #[derive(Deserialize)]
                struct A {
                    host: String,
                    command: String,
                    stdin: Option<String>,
                    timeout_seconds: Option<u64>,
                }
                let a: A = args(a)?;
                if a.command.trim().is_empty() {
                    return Err("command is empty".into());
                }
                Plan::Exec {
                    host: self.host(&a.host)?,
                    command: a.command,
                    stdin: a.stdin,
                    timeout: timeout(a.timeout_seconds),
                }
            }
            "run_snippet" => {
                #[derive(Deserialize)]
                struct A {
                    host: String,
                    snippet: String,
                    #[serde(default)]
                    variables: std::collections::BTreeMap<String, String>,
                    timeout_seconds: Option<u64>,
                }
                let a: A = args(a)?;
                let host = self.host(&a.host)?;
                let snippets = self
                    .app
                    .state::<AppState>()
                    .with_vault(|v| Ok(v.list::<Snippet>()?))
                    .map_err(|e| cmd_err(&e))?;
                let n = a.snippet.trim().to_lowercase();
                let s = snippets
                    .iter()
                    .find(|s| s.id.to_string() == a.snippet.trim())
                    .or_else(|| snippets.iter().find(|s| s.label.to_lowercase() == n))
                    .ok_or_else(|| format!("no snippet '{}'", a.snippet))?;
                let missing: Vec<String> = s
                    .variables()
                    .into_iter()
                    .filter(|v| v.default.is_none() && !a.variables.contains_key(&v.name))
                    .map(|v| v.name)
                    .collect();
                if !missing.is_empty() {
                    return Err(format!("missing variables: {}", missing.join(", ")));
                }
                let values: Vec<(String, String)> = a.variables.into_iter().collect();
                Plan::Exec {
                    host,
                    command: s.render(&values),
                    stdin: None,
                    timeout: timeout(a.timeout_seconds),
                }
            }
            "write_file" => {
                #[derive(Deserialize)]
                struct A {
                    host: String,
                    path: String,
                    content: String,
                    #[serde(default)]
                    encoding: Encoding,
                    #[serde(default)]
                    mode: WriteMode,
                }
                let a: A = args(a)?;
                let data = match a.encoding {
                    Encoding::Text => a.content.into_bytes(),
                    Encoding::Base64 => {
                        use base64::Engine as _;
                        base64::engine::general_purpose::STANDARD
                            .decode(a.content.trim())
                            .map_err(|e| format!("invalid base64: {e}"))?
                    }
                };
                if data.len() > MAX_WRITE_BYTES {
                    return Err(format!("content is larger than {MAX_WRITE_BYTES} bytes"));
                }
                Plan::Write {
                    host: self.host(&a.host)?,
                    path: a.path,
                    data,
                    mode: a.mode,
                }
            }
            "make_directory" => {
                let a: HostPath = args(a)?;
                Plan::Mkdir {
                    host: self.host(&a.host)?,
                    path: a.path,
                }
            }
            "delete_path" => {
                let a: HostPath = args(a)?;
                Plan::Delete {
                    host: self.host(&a.host)?,
                    path: a.path,
                }
            }
            "start_port_forward" | "stop_port_forward" => {
                #[derive(Deserialize)]
                struct A {
                    rule: String,
                }
                let rule = self.rule(&args::<A>(a)?.rule)?;
                if name == "start_port_forward" {
                    Plan::StartForward(rule)
                } else {
                    Plan::StopForward(rule)
                }
            }
            "list_terminals" => Plan::ListTerminals,
            "read_terminal" => {
                #[derive(Deserialize)]
                struct A {
                    terminal: String,
                    commands: Option<usize>,
                    lines: Option<usize>,
                }
                let a: A = args(a)?;
                Plan::ReadTerminal {
                    terminal: self.terminal(&a.terminal)?,
                    commands: a.commands.unwrap_or(5).clamp(1, 20),
                    lines: a.lines.unwrap_or(120).clamp(1, 1000),
                }
            }
            "type_in_terminal" => {
                #[derive(Deserialize)]
                struct A {
                    terminal: String,
                    text: String,
                    #[serde(default)]
                    run: bool,
                    wait_seconds: Option<u64>,
                }
                let a: A = args(a)?;
                if a.text.contains(['\n', '\r']) {
                    return Err("text must be a single line; use run to press Enter".into());
                }
                if a.text.chars().any(|c| c.is_control() && c != '\t') {
                    return Err("text must not contain control characters".into());
                }
                if a.text.trim().is_empty() {
                    return Err("text is empty".into());
                }
                Plan::TypeInTerminal {
                    terminal: self.terminal(&a.terminal)?,
                    text: a.text,
                    run: a.run,
                    wait: Duration::from_secs(a.wait_seconds.unwrap_or(30).min(MAX_TIMEOUT_SECS)),
                }
            }
            "open_terminal" => {
                #[derive(Deserialize)]
                struct A {
                    host: String,
                }
                Plan::OpenTerminal(self.host(&args::<A>(a)?.host)?)
            }
            other => return Err(format!("unknown tool {other}")),
        })
    }

    /// (host, one-line summary, full details for the approval dialog)
    fn describe(plan: &Plan) -> (Option<&Host>, String, String) {
        match plan {
            Plan::ListHosts(q) | Plan::ListSnippets(q) => (
                None,
                q.as_ref()
                    .map_or_else(String::new, |q| format!("query: {q}")),
                String::new(),
            ),
            Plan::ListForwards | Plan::ListTerminals => (None, String::new(), String::new()),
            Plan::ListDir { host, path } => {
                let p = path.clone().unwrap_or_else(|| "~".into());
                (Some(host), p.clone(), p)
            }
            Plan::Read { host, path, .. }
            | Plan::Mkdir { host, path }
            | Plan::Delete { host, path } => (Some(host), path.clone(), path.clone()),
            Plan::Exec {
                host,
                command,
                stdin,
                ..
            } => {
                let mut details = command.clone();
                if let Some(s) = stdin {
                    let (s, _) = shorten(s, 2000);
                    details.push_str("\n\n— stdin —\n");
                    details.push_str(&s);
                }
                (Some(host), command.clone(), details)
            }
            Plan::Write {
                host,
                path,
                data,
                mode,
            } => {
                let verb = match mode {
                    WriteMode::Overwrite => "overwrite",
                    WriteMode::Create => "create",
                    WriteMode::Append => "append to",
                };
                let preview = std::str::from_utf8(data).map_or_else(
                    |_| format!("<{} bytes of binary data>", data.len()),
                    |s| shorten(s, 4000).0,
                );
                (
                    Some(host),
                    format!("{verb} {path} ({} bytes)", data.len()),
                    format!("{verb} {path}\n\n{preview}"),
                )
            }
            Plan::StartForward(r) | Plan::StopForward(r) => {
                (None, r.label.clone(), r.label.clone())
            }
            Plan::OpenTerminal(h) => (Some(h), String::new(), String::new()),
            Plan::ReadTerminal { terminal, .. } => (
                None,
                terminal.info.label.clone(),
                format!(
                    "Read the recent output of the terminal “{}”",
                    terminal.info.label
                ),
            ),
            Plan::TypeInTerminal {
                terminal,
                text,
                run,
                ..
            } => (
                None,
                format!("{}: {text}", terminal.info.label),
                format!(
                    "{} in the terminal “{}”:\n\n{text}",
                    if *run {
                        "Type and run"
                    } else {
                        "Type (without running)"
                    },
                    terminal.info.label
                ),
            ),
        }
    }

    async fn fs(&self, host: &Host) -> Result<Arc<dyn sftp_engine::FileSystem>, String> {
        let files = self.app.state::<FilesState>();
        files
            .fs(
                &self.app,
                &FsSource::Remote {
                    host_id: host.id.to_string(),
                },
            )
            .await
            .map_err(|e| cmd_err(&e))
    }

    async fn resolve_path(fs: &dyn sftp_engine::FileSystem, path: &str) -> Result<String, String> {
        let p = path.trim();
        if p == "~" || p.is_empty() {
            return fs.home().await.map_err(|e| e.to_string());
        }
        if let Some(rest) = p.strip_prefix("~/") {
            let home = fs.home().await.map_err(|e| e.to_string())?;
            return Ok(format!("{}/{rest}", home.trim_end_matches('/')));
        }
        if p.starts_with('/') {
            Ok(p.to_owned())
        } else {
            let home = fs.home().await.map_err(|e| e.to_string())?;
            Ok(format!("{}/{p}", home.trim_end_matches('/')))
        }
    }

    async fn execute(&self, plan: Plan) -> Result<Value, String> {
        match plan {
            Plan::ListHosts(q) => {
                let (hosts, groups) = self.hosts_in_scope()?;
                let list: Vec<Value> = hosts
                    .iter()
                    .filter_map(|h| {
                        let group = group_path(&groups, h.group);
                        let tags = h.tags.join(" ");
                        matches(
                            q.as_deref(),
                            &[&h.label, &h.address, group.as_deref().unwrap_or(""), &tags],
                        )
                        .then(|| self.host_json(h, group))
                    })
                    .collect();
                Ok(json!({ "hosts": list }))
            }
            Plan::ListSnippets(q) => {
                let snippets = self
                    .app
                    .state::<AppState>()
                    .with_vault(|v| Ok(v.list::<Snippet>()?))
                    .map_err(|e| cmd_err(&e))?;
                let list: Vec<Value> = snippets
                    .iter()
                    .filter(|s| {
                        matches(q.as_deref(), &[&s.label, &s.script, &s.tags.join(" ")])
                    })
                    .map(|s| {
                        json!({
                            "id": s.id.to_string(),
                            "label": s.label,
                            "script": s.script,
                            "tags": s.tags,
                            "variables": s.variables().iter().map(|v| json!({"name": v.name, "default": v.default})).collect::<Vec<_>>(),
                        })
                    })
                    .collect();
                Ok(json!({ "snippets": list }))
            }
            Plan::ListForwards => {
                let rules = self
                    .app
                    .state::<AppState>()
                    .with_vault(|v| Ok(v.list::<PortForwardRule>()?))
                    .map_err(|e| cmd_err(&e))?;
                let running = forwarding::forward_status(self.app.state())
                    .await
                    .map_err(|e| cmd_err(&e))?;
                let (hosts, _) = self.hosts_in_scope()?;
                let list: Vec<Value> = rules
                    .iter()
                    .filter_map(|r| {
                        let host = hosts.iter().find(|h| h.id == r.host)?;
                        let status = running.iter().find(|s| s.rule_id == r.id.to_string());
                        Some(json!({
                            "id": r.id.to_string(),
                            "label": r.label,
                            "kind": r.kind,
                            "host": label(host),
                            "bind": format!("{}:{}", r.bind_address, r.bind_port),
                            "destination": r.dest_host.as_ref().map(|d| format!("{d}:{}", r.dest_port.unwrap_or_default())),
                            "running": status.is_some(),
                            "connections": status.map(|s| s.connections),
                        }))
                    })
                    .collect();
                Ok(json!({ "rules": list }))
            }
            Plan::ListDir { host, path } => {
                let fs = self.fs(&host).await?;
                let path = Self::resolve_path(fs.as_ref(), path.as_deref().unwrap_or("~")).await?;
                let mut entries = fs.list(&path).await.map_err(|e| e.to_string())?;
                entries.sort_by(|a, b| a.name.cmp(&b.name));
                let truncated = entries.len() > MAX_LIST_ENTRIES;
                entries.truncate(MAX_LIST_ENTRIES);
                let list: Vec<Value> = entries
                    .iter()
                    .map(|e| {
                        json!({
                            "name": e.name,
                            "type": format!("{:?}", e.kind).to_lowercase(),
                            "size": e.size,
                            "mode": e.mode_string(),
                            "modified": e.modified.and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()),
                        })
                    })
                    .collect();
                Ok(json!({ "path": path, "entries": list, "truncated": truncated }))
            }
            Plan::Read {
                host,
                path,
                max,
                base64,
            } => {
                let fs = self.fs(&host).await?;
                let path = Self::resolve_path(fs.as_ref(), &path).await?;
                let size = fs.stat(&path).await.map_err(|e| e.to_string())?.size;
                let mut reader = fs.open_read(&path, 0).await.map_err(|e| e.to_string())?;
                let mut buf = Vec::new();
                (&mut reader)
                    .take(max)
                    .read_to_end(&mut buf)
                    .await
                    .map_err(|e| e.to_string())?;
                let truncated = size > buf.len() as u64;
                let content = if base64 {
                    use base64::Engine as _;
                    base64::engine::general_purpose::STANDARD.encode(&buf)
                } else {
                    match String::from_utf8(buf) {
                        Ok(s) => s,
                        Err(_) => {
                            return Err(
                                "the file is not UTF-8 text; read it with encoding 'base64'".into(),
                            );
                        }
                    }
                };
                Ok(
                    json!({ "path": path, "size": size, "truncated": truncated, "encoding": if base64 { "base64" } else { "text" }, "content": content }),
                )
            }
            Plan::Exec {
                host,
                command,
                stdin,
                timeout,
            } => {
                let pool = self.app.state::<crate::net::ConnectionPool>();
                let conn = pool
                    .get(&self.app, &host.id.to_string())
                    .await
                    .map_err(|e| cmd_err(&e))?;
                let opts = ssh_engine::ExecOptions {
                    max_output: MAX_EXEC_OUTPUT,
                    stdin: stdin.map(String::into_bytes),
                    timeout: Some(timeout),
                    env: Vec::new(),
                };
                let out = conn
                    .exec_with(&command, opts)
                    .await
                    .map_err(|e| e.to_string())?;
                let (stdout, so) =
                    shorten(&String::from_utf8_lossy(&out.stdout), MAX_RETURNED_OUTPUT);
                let (stderr, se) =
                    shorten(&String::from_utf8_lossy(&out.stderr), MAX_RETURNED_OUTPUT);
                Ok(json!({
                    "host": label(&host),
                    "exit_status": out.exit_status,
                    "exit_signal": out.exit_signal,
                    "stdout": stdout,
                    "stderr": stderr,
                    "truncated": so || se,
                }))
            }
            Plan::Write {
                host,
                path,
                data,
                mode,
            } => {
                let fs = self.fs(&host).await?;
                let path = Self::resolve_path(fs.as_ref(), &path).await?;
                let wm = match mode {
                    WriteMode::Overwrite => sftp_engine::WriteMode::Truncate,
                    WriteMode::Create => sftp_engine::WriteMode::CreateNew,
                    WriteMode::Append => match fs.stat(&path).await {
                        Ok(e) => sftp_engine::WriteMode::Resume { offset: e.size },
                        Err(_) => sftp_engine::WriteMode::Truncate,
                    },
                };
                let mut w = fs.open_write(&path, wm).await.map_err(|e| e.to_string())?;
                w.write_all(&data).await.map_err(|e| e.to_string())?;
                w.shutdown().await.map_err(|e| e.to_string())?;
                Ok(json!({ "path": path, "bytes_written": data.len() }))
            }
            Plan::Mkdir { host, path } => {
                let fs = self.fs(&host).await?;
                let path = Self::resolve_path(fs.as_ref(), &path).await?;
                fs.mkdir(&path).await.map_err(|e| e.to_string())?;
                Ok(json!({ "path": path, "created": true }))
            }
            Plan::Delete { host, path } => {
                let fs = self.fs(&host).await?;
                let path = Self::resolve_path(fs.as_ref(), &path).await?;
                let entry = fs.lstat(&path).await.map_err(|e| e.to_string())?;
                if entry.kind == sftp_engine::EntryKind::Dir {
                    fs.remove_dir(&path).await.map_err(|e| e.to_string())?;
                } else {
                    fs.remove_file(&path).await.map_err(|e| e.to_string())?;
                }
                Ok(json!({ "path": path, "deleted": true }))
            }
            Plan::StartForward(rule) => {
                let st = forwarding::forward_start(
                    self.app.clone(),
                    self.app.state(),
                    self.app.state(),
                    rule.id.to_string(),
                )
                .await
                .map_err(|e| cmd_err(&e))?;
                Ok(json!({ "rule": rule.label, "running": true, "local_address": st.local_addr }))
            }
            Plan::StopForward(rule) => {
                forwarding::forward_stop(self.app.state(), rule.id.to_string())
                    .await
                    .map_err(|e| cmd_err(&e))?;
                Ok(json!({ "rule": rule.label, "running": false }))
            }
            Plan::ListTerminals => {
                let list: Vec<Value> = self
                    .terminals_in_scope()
                    .iter()
                    .map(|t| {
                        let b = t.blocks.lock();
                        let last = b
                            .blocks()
                            .back()
                            .map(|l| json!({ "command": l.command, "exit_status": l.exit }));
                        json!({
                            "id": t.id,
                            "label": t.info.label,
                            "kind": t.info.kind,
                            "host_id": t.info.host_id,
                            "cwd": b.cwd(),
                            "commands_tracked": b.has_integration(),
                            "running": b.running().map(|r| r.command),
                            "last_command": last,
                        })
                    })
                    .collect();
                Ok(json!({ "terminals": list }))
            }
            Plan::ReadTerminal {
                terminal,
                commands,
                lines,
            } => Ok(terminal_json(&terminal, commands, lines)),
            Plan::TypeInTerminal {
                terminal,
                text,
                run,
                wait,
            } => {
                let manager = self.app.state::<crate::sessions::SessionManager>();
                let before = terminal.blocks.lock().blocks().back().map_or(0, |b| b.id);
                let _ = self.app.emit(SHOW_SESSION_EVENT, terminal.id.clone());
                let mut data = text.into_bytes();
                if run {
                    data.push(b'\r');
                }
                manager
                    .send(&terminal.id, crate::sessions::Input::Data(data))
                    .map_err(|e| cmd_err(&e))?;
                if !run {
                    return Ok(json!({ "typed": true, "ran": false }));
                }
                let tracked = terminal.blocks.lock().has_integration();
                if !tracked {
                    tokio::time::sleep(Duration::from_millis(1500)).await;
                    let tail = terminal.blocks.lock().tail(60).join("\n");
                    return Ok(json!({
                        "ran": true,
                        "finished": null,
                        "note": "This shell has no shell integration, so Vaultport cannot tell when the command ends. This is the output so far.",
                        "output": shorten(&tail, MAX_TERMINAL_OUTPUT).0,
                    }));
                }
                let deadline = tokio::time::Instant::now() + wait;
                loop {
                    let done = terminal
                        .blocks
                        .lock()
                        .blocks()
                        .iter()
                        .rev()
                        .find(|b| b.id > before)
                        .cloned();
                    if let Some(b) = done {
                        let (output, cut) = shorten(&b.output, MAX_TERMINAL_OUTPUT);
                        return Ok(json!({
                            "ran": true,
                            "finished": true,
                            "command": b.command,
                            "exit_status": b.exit,
                            "output": output,
                            "output_truncated": cut || b.omitted_lines > 0,
                        }));
                    }
                    if manager.get(&terminal.id).is_none() {
                        return Err("the terminal was closed".into());
                    }
                    if tokio::time::Instant::now() >= deadline {
                        let running = terminal.blocks.lock().running();
                        return Ok(json!({
                            "ran": true,
                            "finished": false,
                            "note": "Still running in the user's terminal; use read_terminal later.",
                            "output_so_far": running.map(|r| shorten(&r.output, MAX_TERMINAL_OUTPUT).0),
                        }));
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
            }
            Plan::OpenTerminal(host) => {
                self.app
                    .emit(OPEN_TERMINAL_EVENT, host.id.to_string())
                    .map_err(|e| e.to_string())?;
                #[cfg(desktop)]
                if let Some(w) = self.app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
                Ok(json!({ "opened": label(&host) }))
            }
        }
    }

    fn host_json(&self, h: &Host, group: Option<String>) -> Value {
        let _ = self;
        let port = h.config.ssh.port.unwrap_or(core_model::Defaults::SSH_PORT);
        let username = match &h.config.ssh.credential {
            Some(core_model::Credential::Inline { username, .. }) => username.clone(),
            _ => None,
        };
        json!({
            "id": h.id.to_string(),
            "label": label(h),
            "address": h.address,
            "port": port,
            "username": username,
            "group": group,
            "tags": h.tags,
            "os": h.os,
            "favorite": h.favorite,
            "mosh": h.config.mosh.enabled.unwrap_or(false),
        })
    }
}

#[async_trait::async_trait]
impl mcp_server::Handler for AgentHandler {
    fn tools(&self) -> Vec<Tool> {
        let read_only = self.state.settings().read_only;
        DEFS.iter()
            .filter(|d| !(read_only && d.changes))
            .map(|d| Tool {
                name: d.name.into(),
                title: Some(d.title.into()),
                description: d.description.into(),
                input_schema: (d.schema)(),
                output_schema: None,
                annotations: Some(ToolAnnotations {
                    title: Some(d.title.into()),
                    read_only_hint: Some(!d.changes),
                    destructive_hint: Some(d.destructive),
                    idempotent_hint: Some(!d.changes),
                    open_world_hint: Some(true),
                }),
            })
            .collect()
    }

    async fn call(&self, ctx: CallContext, name: String, arguments: Value) -> ToolResult {
        let client = ctx.client.display_name();
        self.state.connected_name(ctx.connection_id, &client);
        let settings = self.state.settings();
        let Some(def) = DEFS.iter().find(|d| d.name == name) else {
            return ToolResult::error(format!("unknown tool {name}"));
        };
        if !settings.enabled {
            return ToolResult::error("agent access was turned off by the user");
        }
        if settings.read_only && def.changes {
            return ToolResult::error("Vaultport only allows read-only tools for agents");
        }
        let mut entry = AuditEntry {
            id: uuid::Uuid::now_v7().to_string(),
            at: now_ms(),
            client: client.clone(),
            tool: name.clone(),
            host: None,
            summary: String::new(),
            outcome: AuditOutcome::Error,
            message: None,
        };
        // Do not ask the user to approve something that cannot run.
        if let Err(e) = self.app.state::<AppState>().with_vault(|_| Ok(())) {
            let msg = cmd_err(&e);
            entry.message = Some(msg.clone());
            self.state.record(&self.app, entry);
            return ToolResult::error(msg);
        }
        let plan = match self.plan(&name, arguments) {
            Ok(p) => p,
            Err(e) => {
                entry.message = Some(e.clone());
                self.state.record(&self.app, entry);
                return ToolResult::error(e);
            }
        };
        let (host, summary, details) = Self::describe(&plan);
        entry.host = host.map(label);
        entry.summary.clone_from(&summary);
        let needs_approval = def.changes || settings.approval == ApprovalPolicy::Always;
        if needs_approval {
            let request = AgentApprovalRequest {
                id: String::new(),
                client,
                tool: name.clone(),
                title: def.title.into(),
                host: entry.host.clone(),
                details,
                destructive: def.destructive,
            };
            let host_id = host.map(|h| h.id.to_string());
            if !self
                .state
                .approve(&self.app, ctx.connection_id, request, host_id.as_deref())
                .await
            {
                entry.outcome = AuditOutcome::Denied;
                self.state.record(&self.app, entry);
                return ToolResult::error(
                    "The user denied this action in Vaultport. Do not retry it unchanged.",
                );
            }
        }
        match Box::pin(self.execute(plan)).await {
            Ok(v) => {
                entry.outcome = AuditOutcome::Ok;
                self.state.record(&self.app, entry);
                ToolResult::json(v)
            }
            Err(e) => {
                entry.message = Some(e.clone());
                self.state.record(&self.app, entry);
                ToolResult::error(e)
            }
        }
    }

    fn disconnected(&self, connection_id: u64) {
        self.state.disconnected(connection_id);
    }
}

/// The recent commands (or lines) of a terminal for read_terminal.
fn terminal_json(t: &Terminal, commands: usize, lines: usize) -> Value {
    let b = t.blocks.lock();
    let mut budget = MAX_TERMINAL_OUTPUT;
    let mut list: Vec<Value> = Vec::new();
    // Newest first while spending the budget, then back to chronological order.
    for block in b.recent(commands).iter().rev() {
        let (output, cut) = shorten(&block.output, budget.max(200));
        budget = budget.saturating_sub(output.len());
        list.push(json!({
            "command": block.command,
            "cwd": block.cwd,
            "exit_status": block.exit,
            "finished": block.finished_at.is_some(),
            "output": output,
            "output_truncated": cut || block.omitted_lines > 0,
        }));
    }
    list.reverse();
    let tail = if b.has_integration() {
        None
    } else {
        Some(shorten(&b.tail(lines).join("\n"), MAX_TERMINAL_OUTPUT).0)
    };
    json!({
        "terminal": t.id,
        "label": t.info.label,
        "cwd": b.cwd(),
        "commands_tracked": b.has_integration(),
        "commands": list,
        "output": tail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_definitions_are_valid() {
        let mut names = std::collections::HashSet::new();
        for d in DEFS {
            assert!(names.insert(d.name), "duplicate tool {}", d.name);
            assert!(
                d.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                d.name
            );
            let s = (d.schema)();
            assert_eq!(s["type"], "object");
            for r in s["required"].as_array().into_iter().flatten() {
                assert!(
                    s["properties"]
                        .get(r.as_str().unwrap_or_default())
                        .is_some()
                );
            }
            // Anything destructive must also count as a change (and so need approval).
            assert!(!d.destructive || d.changes, "{}", d.name);
        }
    }

    #[test]
    fn long_output_keeps_head_and_tail() {
        let text = (0..1000).fold(String::new(), |mut s, i| {
            use std::fmt::Write as _;
            let _ = writeln!(s, "{i}");
            s
        });
        let (s, cut) = shorten(&text, 300);
        assert!(cut);
        assert!(s.starts_with("0\n1\n"));
        assert!(s.ends_with("999\n"));
        assert!(s.contains("characters omitted"));
        assert_eq!(shorten("short", 300), ("short".to_owned(), false));
    }

    #[test]
    fn query_matching_is_case_insensitive() {
        assert!(matches(Some("PROD"), &["web-prod-1"]));
        assert!(matches(None, &["x"]));
        assert!(matches(Some("  "), &["x"]));
        assert!(!matches(Some("db"), &["web"]));
    }
}
