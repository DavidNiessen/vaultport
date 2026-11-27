// SPDX-License-Identifier: GPL-3.0-or-later
// Prevents an additional console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `ssh-client mcp`: stdio MCP server for AI agents, relaying to the running app.
    #[cfg(desktop)]
    if std::env::args_os().nth(1).is_some_and(|a| a == "mcp") {
        std::process::exit(ssh_client_lib::agent::proxy::run());
    }
    ssh_client_lib::run();
}
