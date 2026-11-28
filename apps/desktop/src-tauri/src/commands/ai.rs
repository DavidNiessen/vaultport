// SPDX-License-Identifier: GPL-3.0-or-later
//! Optional local AI command generation. The model runs on this device; the only network
//! access is the one-time, hash-verified model download when the user enables the feature.

use crate::error::{CmdError, CmdResult, ErrorCode};
use local_ai::{CancellationToken, CommandContext, EngineOptions, ModelManager, ModelStatus};
use parking_lot::Mutex;
use serde::Serialize;
use specta::Type;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Copy, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiState {
    NotInstalled,
    Downloading,
    Installed,
    Loading,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub state: AiState,
    pub model_name: String,
    pub license: String,
    pub downloaded: f64,
    pub total: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiSuggestion {
    pub command: String,
    pub explanation: String,
    pub risk: String,
}

/// Local AI state shared by the commands.
pub struct AiManager {
    manager: Option<Arc<ModelManager>>,
    cancel: Mutex<Option<CancellationToken>>,
    progress: Arc<Mutex<Option<(u64, u64)>>>,
    error: Arc<Mutex<Option<String>>>,
}

impl AiManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        Self {
            manager: ModelManager::new(data_dir).ok().map(Arc::new),
            cancel: Mutex::new(None),
            progress: Arc::default(),
            error: Arc::default(),
        }
    }

    fn manager(&self) -> CmdResult<Arc<ModelManager>> {
        self.manager
            .clone()
            .ok_or_else(|| CmdError::new(ErrorCode::Unsupported, "local AI is unavailable"))
    }
}

fn model() -> &'static local_ai::ModelEntry {
    ModelManager::default_model()
}

/// Model installation status.
#[tauri::command]
#[specta::specta]
pub fn ai_status(ai: State<'_, AiManager>) -> AiStatus {
    let m = model();
    let mut status = AiStatus {
        state: AiState::NotInstalled,
        model_name: m.display_name.to_owned(),
        license: m.license.to_owned(),
        downloaded: 0.0,
        total: m.size as f64,
        error: ai.error.lock().clone(),
    };
    if !local_ai::is_available() {
        status.state = AiState::Error;
        status.error = Some("this build was compiled without local AI".into());
        return status;
    }
    if let Some((done, total)) = *ai.progress.lock() {
        status.state = AiState::Downloading;
        status.downloaded = done as f64;
        status.total = total as f64;
        return status;
    }
    if let Ok(mgr) = ai.manager() {
        status.state = match mgr.status(m) {
            ModelStatus::NotInstalled => AiState::NotInstalled,
            ModelStatus::Downloading { progress } => {
                status.downloaded = progress.downloaded as f64;
                AiState::Downloading
            }
            ModelStatus::Installed { .. } => AiState::Installed,
            ModelStatus::Loading => AiState::Loading,
            ModelStatus::Ready => AiState::Ready,
            ModelStatus::Error { message } => {
                status.error = Some(message);
                AiState::Error
            }
            #[allow(unreachable_patterns)]
            _ => AiState::Error,
        };
    }
    status
}

/// Starts downloading and verifying the model in the background.
#[tauri::command]
#[specta::specta]
pub fn ai_install(ai: State<'_, AiManager>) -> CmdResult<()> {
    let mgr = ai.manager()?;
    if ai.progress.lock().is_some() {
        return Ok(());
    }
    let token = CancellationToken::new();
    *ai.cancel.lock() = Some(token.clone());
    *ai.progress.lock() = Some((0, model().size));
    *ai.error.lock() = None;
    let progress = ai.progress.clone();
    let error = ai.error.clone();
    tauri::async_runtime::spawn(async move {
        let p = progress.clone();
        let result = mgr
            .install(
                model(),
                move |pr| {
                    *p.lock() = Some((pr.downloaded, pr.total));
                },
                &token,
            )
            .await;
        *progress.lock() = None;
        if let Err(e) = result
            && !token.is_cancelled()
        {
            *error.lock() = Some(e.to_string());
        }
    });
    Ok(())
}

/// Cancels a running download (the partial file is kept for resuming).
#[tauri::command]
#[specta::specta]
pub fn ai_cancel(ai: State<'_, AiManager>) {
    if let Some(t) = ai.cancel.lock().take() {
        t.cancel();
    }
}

/// Deletes the downloaded model.
#[tauri::command]
#[specta::specta]
pub fn ai_remove(ai: State<'_, AiManager>) -> CmdResult<()> {
    let mgr = ai.manager()?;
    mgr.unload();
    mgr.remove(model())
        .map_err(|e| CmdError::internal(e.to_string()))
}

async fn engine(ai: &AiManager) -> CmdResult<Arc<local_ai::Engine>> {
    let mgr = ai.manager()?;
    if let Some(e) = mgr.engine() {
        return Ok(e);
    }
    mgr.load(model(), EngineOptions::default())
        .await
        .map_err(|e| CmdError::new(ErrorCode::Unsupported, e.to_string()))
}

fn risk(command: &str) -> String {
    use term_intel::risk::RiskLevel;
    match term_intel::risk::assess_risk(command).level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        _ => "critical",
    }
    .to_owned()
}

/// Context about a terminal session for the model: its host's OS, the working directory and
/// the most recent commands.
fn session_context(app: &tauri::AppHandle, session_id: Option<&str>) -> CommandContext {
    use tauri::Manager;
    let mut ctx = CommandContext::default();
    let Some((info, blocks)) =
        session_id.and_then(|id| app.state::<crate::sessions::SessionManager>().get(id))
    else {
        return ctx;
    };
    {
        let b = blocks.lock();
        ctx.cwd = b.cwd().map(str::to_owned);
        ctx.recent_commands = b
            .recent(8)
            .into_iter()
            .map(|x| x.command)
            .filter(|c| !c.trim().is_empty())
            .collect();
    }
    if info.kind == "local" {
        ctx.os = Some(std::env::consts::OS.replace("macos", "macOS"));
        ctx.shell = std::env::var("SHELL")
            .ok()
            .and_then(|s| s.rsplit('/').next().map(str::to_owned));
    } else if let Some(host) = info.host_id.as_deref().and_then(|h| {
        app.state::<crate::state::AppState>()
            .with_vault(|v| Ok(v.get::<core_model::Host>(h)?))
            .ok()
            .flatten()
    }) {
        let os = format!("{:?}", host.os);
        if os != "Unknown" {
            ctx.os = Some(os);
        }
    }
    ctx
}

async fn suggest(
    ai: &AiManager,
    prompt: String,
    context: CommandContext,
) -> CmdResult<AiSuggestion> {
    let engine = engine(ai).await?;
    let request = local_ai::GenerateRequest { prompt, context };
    let s = engine
        .generate(&request, &CancellationToken::new())
        .await
        .map_err(|e| CmdError::internal(e.to_string()))?;
    Ok(AiSuggestion {
        risk: risk(&s.command),
        command: s.command,
        explanation: s.explanation,
    })
}

/// Generates one shell command for a natural-language request. Never executes anything.
/// With `session_id`, the terminal's OS, directory and recent commands are given as context.
#[tauri::command]
#[specta::specta]
pub async fn ai_generate_command(
    app: tauri::AppHandle,
    ai: State<'_, AiManager>,
    prompt: String,
    session_id: Option<String>,
    selection: Option<String>,
) -> CmdResult<AiSuggestion> {
    let mut context = session_context(&app, session_id.as_deref());
    context.selection = selection.filter(|s| !s.trim().is_empty());
    suggest(&ai, prompt, context).await
}

/// Lines of a failed command's output given to the model.
const FIX_OUTPUT_LINES: usize = 40;

/// Suggests a corrected command for the most recent command of a terminal that failed (by
/// its exit status), from the command and the end of its output. Never executes anything.
#[tauri::command]
#[specta::specta]
pub async fn ai_fix_command(
    app: tauri::AppHandle,
    ai: State<'_, AiManager>,
    session_id: String,
) -> CmdResult<AiSuggestion> {
    use tauri::Manager;
    let (_, blocks) = app
        .state::<crate::sessions::SessionManager>()
        .get(&session_id)
        .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "session not found"))?;
    let failed = blocks
        .lock()
        .blocks()
        .iter()
        .rev()
        .take(5)
        .find(|b| b.exit.is_some_and(|e| e != 0))
        .cloned()
        .ok_or_else(|| CmdError::new(ErrorCode::NotFound, "no failed command to fix"))?;
    let lines: Vec<&str> = failed.output.lines().collect();
    let tail = lines[lines.len().saturating_sub(FIX_OUTPUT_LINES)..].join("\n");
    let prompt = format!(
        "This command failed with exit status {}: {}\nFix it: give the corrected command.",
        failed.exit.unwrap_or(1),
        failed.command
    );
    let mut context = session_context(&app, Some(&session_id));
    context.selection = Some(tail).filter(|t| !t.trim().is_empty());
    suggest(&ai, prompt, context).await
}

/// Generates a snippet script from a description.
#[tauri::command]
#[specta::specta]
pub async fn ai_generate_snippet(
    ai: State<'_, AiManager>,
    description: String,
) -> CmdResult<String> {
    let engine = engine(&ai).await?;
    let s = engine
        .generate_snippet(
            &local_ai::SnippetRequest {
                description,
                context: CommandContext::default(),
            },
            &CancellationToken::new(),
        )
        .await
        .map_err(|e| CmdError::internal(e.to_string()))?;
    Ok(s.script)
}
