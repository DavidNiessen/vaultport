// SPDX-License-Identifier: GPL-3.0-or-later
//! Where the agent socket and its token live, shared by the app and the `mcp` relay.

use std::io;
use std::path::{Path, PathBuf};

use super::AgentClientConfig;

/// The app identifier from `tauri.conf.json` (the app data directory name).
pub const IDENTIFIER: &str = "dev.niessen.vaultport";
pub const SETTINGS_FILE: &str = "agent.json";
pub const TOKEN_FILE: &str = "agent-token";
#[cfg(unix)]
const SOCKET_FILE: &str = "agent.sock";

/// The app data directory as Tauri computes it, for the relay (which runs without Tauri).
pub fn default_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join(IDENTIFIER))
}

/// Unix socket paths are limited to ~104 bytes (`sun_path`).
#[cfg(unix)]
const MAX_SOCKET_PATH: usize = 100;

/// The agent socket: in the per-user runtime directory when there is one (Linux), else in the
/// app data directory, else (long paths) in a private per-user temporary directory.
#[cfg(unix)]
pub fn socket_path(data_dir: &Path) -> PathBuf {
    if let Some(rt) = dirs::runtime_dir() {
        let p = rt.join(format!("{IDENTIFIER}.agent.sock"));
        if p.as_os_str().len() <= MAX_SOCKET_PATH {
            return p;
        }
    }
    let p = data_dir.join(SOCKET_FILE);
    if p.as_os_str().len() <= MAX_SOCKET_PATH {
        return p;
    }
    private_temp_dir(data_dir).join(SOCKET_FILE)
}

/// `$TMPDIR/vaultport-<hash of the data dir>`; the short name keeps the socket path short.
#[cfg(unix)]
fn private_temp_dir(data_dir: &Path) -> PathBuf {
    use std::hash::{Hash as _, Hasher as _};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data_dir.hash(&mut h);
    let base = std::env::temp_dir();
    let base = if base.as_os_str().len() > 60 {
        PathBuf::from("/tmp")
    } else {
        base
    };
    base.join(format!("vaultport-{:012x}", h.finish() & 0xffff_ffff_ffff))
}

/// Makes sure the socket's directory exists and only the current user can enter it.
#[cfg(unix)]
pub fn prepare_socket_dir(data_dir: &Path, socket: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};
    let Some(dir) = socket.parent() else {
        return Ok(());
    };
    if dir == data_dir || dirs::runtime_dir().is_some_and(|r| r == dir) {
        return Ok(());
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    // In a shared temp directory someone else could have created it first: verify.
    let meta = std::fs::symlink_metadata(dir)?;
    let owner = std::fs::metadata(data_dir)?.uid();
    if !meta.is_dir() || meta.uid() != owner {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{} is not owned by the current user", dir.display()),
        ));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// The per-user named pipe.
#[cfg(windows)]
pub fn pipe_name() -> String {
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    format!(r"\\.\pipe\{IDENTIFIER}.agent.{user}")
}

/// Writes a file only the current user can read.
pub fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp)?;
        std::io::Write::write_all(&mut f, bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Creates (or replaces) the access token.
pub fn create_token(data_dir: &Path) -> io::Result<String> {
    let mut raw = [0u8; 32];
    getrandom::fill(&mut raw).map_err(|e| io::Error::other(e.to_string()))?;
    let token = hex(&raw);
    write_private(&data_dir.join(TOKEN_FILE), token.as_bytes())?;
    Ok(token)
}

/// Reads the token, creating one if missing.
pub fn load_or_create_token(data_dir: &Path) -> io::Result<String> {
    match read_token(data_dir) {
        Ok(t) => Ok(t),
        Err(e) if e.kind() == io::ErrorKind::NotFound => create_token(data_dir),
        Err(e) => Err(e),
    }
}

pub fn read_token(data_dir: &Path) -> io::Result<String> {
    let t = std::fs::read_to_string(data_dir.join(TOKEN_FILE))?;
    let t = t.trim().to_owned();
    if t.len() != 64 || !t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "corrupt agent token",
        ));
    }
    Ok(t)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Compares in constant time.
pub fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Ready-to-paste setup for common MCP clients.
pub fn client_configs(exe: &str) -> Vec<AgentClientConfig> {
    let quoted = shell_quote(exe);
    let desktop = serde_json::json!({
        "mcpServers": { "vaultport": { "command": exe, "args": ["mcp"] } }
    });
    let vscode = serde_json::json!({
        "servers": { "vaultport": { "type": "stdio", "command": exe, "args": ["mcp"] } }
    });
    let pretty = |v: &serde_json::Value| serde_json::to_string_pretty(v).unwrap_or_default();
    vec![
        AgentClientConfig {
            client: "claude-code".into(),
            kind: "shell".into(),
            location: None,
            text: format!("claude mcp add --scope user vaultport -- {quoted} mcp"),
        },
        AgentClientConfig {
            client: "claude-desktop".into(),
            kind: "json".into(),
            location: Some("claude_desktop_config.json".into()),
            text: pretty(&desktop),
        },
        AgentClientConfig {
            client: "cursor".into(),
            kind: "json".into(),
            location: Some("~/.cursor/mcp.json".into()),
            text: pretty(&desktop),
        },
        AgentClientConfig {
            client: "vscode".into(),
            kind: "json".into(),
            location: Some(".vscode/mcp.json".into()),
            text: pretty(&vscode),
        },
        AgentClientConfig {
            client: "codex".into(),
            kind: "shell".into(),
            location: None,
            text: format!("codex mcp add vaultport -- {quoted} mcp"),
        },
        AgentClientConfig {
            client: "generic".into(),
            kind: "shell".into(),
            location: None,
            text: format!("{quoted} mcp"),
        },
    ]
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-:\\".contains(c))
    {
        s.to_owned()
    } else if cfg!(windows) {
        format!("\"{s}\"")
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_matches_tauri_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap_or_default();
        assert_eq!(conf["identifier"], IDENTIFIER);
    }

    #[test]
    fn tokens_are_private_random_and_compared_in_constant_time() {
        let dir = std::env::temp_dir().join(format!("agent-token-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&dir).unwrap_or_default();
        let a = load_or_create_token(&dir).unwrap_or_default();
        assert_eq!(a.len(), 64);
        assert_eq!(read_token(&dir).unwrap_or_default(), a);
        let b = create_token(&dir).unwrap_or_default();
        assert_ne!(a, b);
        assert!(token_eq(&b, &read_token(&dir).unwrap_or_default()));
        assert!(!token_eq(&a, &b));
        assert!(!token_eq(&a, &a[..10]));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = std::fs::metadata(dir.join(TOKEN_FILE))
                .map(|m| m.permissions().mode())
                .unwrap_or_default();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn socket_paths_stay_short() {
        let long = PathBuf::from(format!(
            "/home/{}/.local/share/{IDENTIFIER}",
            "x".repeat(120)
        ));
        assert!(socket_path(&long).as_os_str().len() <= MAX_SOCKET_PATH);
        assert_eq!(socket_path(&long), socket_path(&long));
    }

    #[test]
    fn configs_quote_paths() {
        let c = client_configs("/Applications/ssh client.app/Contents/MacOS/vaultport");
        assert!(c[0].text.contains("-- '/Applications/ssh client.app"));
        let json: serde_json::Value = serde_json::from_str(&c[1].text).unwrap_or_default();
        assert_eq!(json["mcpServers"]["vaultport"]["args"][0], "mcp");
        assert_eq!(
            client_configs("/usr/bin/vaultport")[5].text,
            "/usr/bin/vaultport mcp"
        );
    }
}
