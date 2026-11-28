// SPDX-License-Identifier: GPL-3.0-or-later
//! Vaultport application core.
//!
//! The Rust side owns every secret and every network connection. The web UI talks to it only
//! through the typed commands registered below (bindings are generated with tauri-specta).

#![forbid(unsafe_code)]
// Tauri commands receive owned arguments and keep uniform `Result` signatures by design;
// several matched enums come from `#[non_exhaustive]` engine crates.
#![allow(
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::match_wildcard_for_single_variants,
    clippy::items_after_statements
)]

pub mod agent;
mod commands;
mod error;
mod keystore;
mod mobile;
mod net;
mod prefs;
mod sessions;
#[cfg(desktop)]
mod sleep;
mod state;

use commands::{
    agent as agent_cmd, ai, complete, data, entities, files, forwarding, keys, logs, platform,
    sessions as session_cmd, sync as sync_cmd, update, vault as vault_cmd,
};
use state::AppState;
use tauri::Manager;
use tauri_specta::{Builder, collect_commands};

/// Builds the typed command registry. Also used by the binding generator test.
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            platform::platform_info,
            vault_cmd::vault_status,
            vault_cmd::vault_create,
            vault_cmd::vault_unlock,
            vault_cmd::vault_lock,
            vault_cmd::vault_set_password,
            vault_cmd::vault_remove_password,
            vault_cmd::vault_set_device_unlock,
            entities::list_hosts,
            entities::save_host,
            entities::delete_host,
            entities::resolve_host,
            entities::list_groups,
            entities::save_group,
            entities::delete_group,
            entities::list_identities,
            entities::save_identity,
            entities::delete_identity,
            entities::list_keys,
            entities::save_key_meta,
            entities::delete_key_meta,
            entities::list_snippets,
            entities::save_snippet,
            entities::delete_snippet,
            entities::list_packages,
            entities::save_package,
            entities::delete_package,
            entities::list_forwards,
            entities::save_forward,
            entities::delete_forward,
            entities::list_known_hosts,
            entities::save_known_host,
            entities::delete_known_host,
            entities::list_workspaces,
            entities::save_workspace,
            entities::delete_workspace,
            entities::store_secret,
            entities::delete_secret,
            entities::get_settings,
            entities::save_settings,
            session_cmd::session_open,
            session_cmd::session_write,
            session_cmd::session_resize,
            session_cmd::session_close,
            session_cmd::session_respond,
            keys::key_generate,
            keys::key_import,
            keys::read_key_file,
            keys::key_enroll_fido2,
            keys::key_create_hardware,
            data::default_ssh_config_path,
            data::import_file,
            data::cloud_add_integration,
            data::cloud_sync,
            sync_cmd::sync_list_targets,
            sync_cmd::sync_add_target,
            sync_cmd::sync_remove_target,
            sync_cmd::sync_now,
            sync_cmd::sync_export_file,
            sync_cmd::sync_import_file,
            sync_cmd::sync_start_pairing,
            sync_cmd::sync_accept_pairing,
            logs::list_logs,
            logs::log_bookmark,
            logs::log_delete,
            logs::log_read,
            logs::log_export,
            logs::history_list,
            complete::term_complete,
            complete::term_commands,
            complete::term_context,
            complete::term_integration_bootstrap,
            files::fs_list,
            files::fs_mkdir,
            files::fs_rename,
            files::fs_chmod,
            files::fs_remove,
            files::fs_edit,
            files::fs_download,
            files::transfer_start,
            files::transfer_list,
            files::transfer_cancel,
            files::transfer_clear_finished,
            forwarding::forward_status,
            forwarding::forward_start,
            forwarding::forward_stop,
            ai::ai_status,
            ai::ai_install,
            ai::ai_cancel,
            ai::ai_remove,
            ai::ai_generate_command,
            ai::ai_fix_command,
            ai::ai_generate_snippet,
            update::update_check,
            update::update_install,
            net::prompt_respond,
            keys::key_export_to_host,
            files::fs_upload_edit,
            session_cmd::serial_list_ports,
            agent_cmd::agent_status,
            agent_cmd::agent_save_settings,
            agent_cmd::agent_respond,
            agent_cmd::agent_audit_log,
            agent_cmd::agent_rotate_token,
            agent_cmd::agent_disconnect_all,
        ])
        // Sent as event payloads rather than command results.
        .typ::<agent::AgentApprovalRequest>()
}

/// Path of the generated TypeScript bindings, relative to the crate root.
pub const BINDINGS_PATH: &str = "../src/lib/ipc/bindings.ts";

/// Writes the TypeScript bindings for all commands.
pub fn export_bindings(path: &str) -> Result<(), String> {
    specta_builder()
        .export(
            specta_typescript::Typescript::default().header("// @ts-nocheck\n/* eslint-disable */\n// SPDX-License-Identifier: GPL-3.0-or-later\n// Generated by tauri-specta. Do not edit by hand: run `cargo test -p vaultport export_bindings`."),
            path,
        )
        .map_err(|e| e.to_string())
}

#[cfg(desktop)]
fn configure_window(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        // We draw our own title bar on Windows and Linux.
        if std::env::var_os("VAULTPORT_NATIVE_DECORATIONS").is_none() {
            let _ = window.set_decorations(false);
        }
    }
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{NSVisualEffectMaterial, apply_vibrancy};
        let _ = apply_vibrancy(&window, NSVisualEffectMaterial::Sidebar, None, None);
    }
    #[cfg(target_os = "windows")]
    {
        let _ = window_vibrancy::apply_mica(&window, None);
    }
    let _ = window.show();
}

/// Runs the application.
#[allow(clippy::missing_panics_doc)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    if let Err(e) = export_bindings(BINDINGS_PATH) {
        eprintln!("failed to export TypeScript bindings: {e}");
    }

    // Desktop adds plugins below; mobile does not.
    #[cfg_attr(mobile, allow(unused_mut))]
    let mut app = tauri::Builder::default();

    #[cfg(desktop)]
    {
        app = app
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.set_focus();
                }
            }))
            .plugin(
                tauri_plugin_window_state::Builder::new()
                    .with_state_flags(
                        tauri_plugin_window_state::StateFlags::all()
                            & !tauri_plugin_window_state::StateFlags::DECORATIONS
                            & !tauri_plugin_window_state::StateFlags::VISIBLE,
                    )
                    .build(),
            )
            .plugin(tauri_plugin_updater::Builder::new().build());
    }

    app.plugin(
        tauri_plugin_log::Builder::new()
            .level(if cfg!(debug_assertions) {
                log::LevelFilter::Debug
            } else {
                log::LevelFilter::Info
            })
            .build(),
    )
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_clipboard_manager::init())
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_vaultport_mobile::init())
    .invoke_handler(builder.invoke_handler())
    .setup(move |app| {
        builder.mount_events(app);
        let data_dir = app.path().app_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;
        app.manage(AppState::new(data_dir.clone()));
        app.manage(sessions::SessionManager::default());
        app.manage(net::PromptBroker::default());
        app.manage(net::ConnectionPool::default());
        app.manage(forwarding::ForwardManager::default());
        app.manage(files::FilesState::new());
        app.manage(complete::CompletionState::default());
        complete::watch_vault(app.handle());
        app.manage(ai::AiManager::new(&app.path().app_data_dir()?));
        mobile::init(app.handle());
        app.manage(std::sync::Arc::new(agent::AgentState::new(&data_dir)));
        agent::start(app.handle());
        #[cfg(desktop)]
        {
            configure_window(app);
            sleep::start(app.handle());
        }
        Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running vaultport");
}

#[cfg(test)]
mod tests {
    /// Regenerates the TypeScript bindings. CI fails if the committed file is stale.
    #[test]
    fn export_bindings() {
        super::export_bindings(super::BINDINGS_PATH).unwrap_or_else(|e| panic!("{e}"));
    }
}
