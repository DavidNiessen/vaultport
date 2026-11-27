// SPDX-License-Identifier: GPL-3.0-or-later
//! Model lifecycle: install, verify, load, unload, remove.
//!
//! Models are stored under `<app data dir>/local-ai/models/<file name>`; partial downloads sit
//! next to them as `<file name>.part`. The directory is provided by the caller (the Tauri app
//! passes its platform app-data directory), nothing is written anywhere else.
//!
//! At most one engine is loaded at a time to bound memory use; loading another model replaces
//! it.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use crate::catalog::{self, ModelEntry};
use crate::download::{DownloadSpec, Downloader, Progress, part_path, verify_file};
use crate::engine::{Engine, EngineOptions};
use crate::error::{Error, Result};

/// User-visible state of one model.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModelStatus {
    /// No complete file on disk.
    NotInstalled,
    /// A download is running.
    Downloading {
        /// Latest progress report.
        progress: Progress,
    },
    /// The verified file is on disk.
    Installed {
        /// Location of the GGUF file.
        path: PathBuf,
        /// Its size in bytes.
        size: u64,
    },
    /// The model is being loaded into memory.
    Loading,
    /// The model is loaded and can generate.
    Ready,
    /// The last install or load failed.
    Error {
        /// Human readable error.
        message: String,
    },
}

#[derive(Debug, Clone)]
enum Transient {
    Downloading(Progress),
    Loading,
    Failed(String),
}

#[derive(Debug, Default)]
struct State {
    transient: HashMap<&'static str, Transient>,
    engine: Option<(&'static str, Arc<Engine>)>,
}

/// Owns the model directory and the loaded engine.
#[derive(Debug)]
pub struct ModelManager {
    dir: PathBuf,
    downloader: Downloader,
    state: Arc<Mutex<State>>,
    /// Replaces the catalog URL (unit tests point it at a mock server).
    url_override: Option<String>,
}

/// Clears an in-progress marker if the operation's future is dropped half-way.
struct BusyGuard {
    state: Arc<Mutex<State>>,
    id: &'static str,
    armed: bool,
}

impl BusyGuard {
    fn finish(mut self, outcome: Option<Transient>) {
        self.armed = false;
        let mut state = lock(&self.state);
        match outcome {
            Some(t) => state.transient.insert(self.id, t),
            None => state.transient.remove(self.id),
        };
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        if self.armed {
            lock(&self.state).transient.remove(self.id);
        }
    }
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

impl ModelManager {
    /// A manager storing models below `app_data_dir/local-ai/models`.
    pub fn new(app_data_dir: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::with_downloader(app_data_dir, Downloader::new()?))
    }

    /// Like [`Self::new`] with a custom downloader (e.g. a different host policy).
    pub fn with_downloader(app_data_dir: impl AsRef<Path>, downloader: Downloader) -> Self {
        Self {
            dir: app_data_dir.as_ref().join("local-ai").join("models"),
            downloader,
            state: Arc::default(),
            url_override: None,
        }
    }

    /// The recommended model for this platform (0.5B on Android/iOS, 1.5B elsewhere).
    pub fn default_model() -> &'static ModelEntry {
        catalog::default_model()
    }

    /// Directory holding the model files.
    pub fn models_dir(&self) -> &Path {
        &self.dir
    }

    /// Where `model` is (or will be) stored.
    pub fn model_path(&self, model: &ModelEntry) -> PathBuf {
        self.dir.join(model.file_name)
    }

    fn installed(&self, model: &ModelEntry) -> Option<(PathBuf, u64)> {
        let path = self.model_path(model);
        let meta = std::fs::metadata(&path).ok()?;
        (meta.is_file() && meta.len() == model.size).then_some((path, meta.len()))
    }

    /// Current status of `model`. Cheap: only `stat`s the file.
    pub fn status(&self, model: &ModelEntry) -> ModelStatus {
        {
            let state = lock(&self.state);
            if state.engine.as_ref().is_some_and(|(id, _)| *id == model.id) {
                return ModelStatus::Ready;
            }
            match state.transient.get(model.id) {
                Some(Transient::Downloading(progress)) => {
                    return ModelStatus::Downloading {
                        progress: *progress,
                    };
                }
                Some(Transient::Loading) => return ModelStatus::Loading,
                Some(Transient::Failed(message)) => {
                    return ModelStatus::Error {
                        message: message.clone(),
                    };
                }
                None => {}
            }
        }
        match self.installed(model) {
            Some((path, size)) => ModelStatus::Installed { path, size },
            None => ModelStatus::NotInstalled,
        }
    }

    fn begin(&self, model: &'static ModelEntry, marker: Transient) -> Result<BusyGuard> {
        let mut state = lock(&self.state);
        if matches!(
            state.transient.get(model.id),
            Some(Transient::Downloading(_) | Transient::Loading)
        ) {
            return Err(Error::Busy(model.id.to_owned()));
        }
        state.transient.insert(model.id, marker);
        Ok(BusyGuard {
            state: self.state.clone(),
            id: model.id,
            armed: true,
        })
    }

    fn spec(&self, model: &ModelEntry) -> DownloadSpec {
        let mut spec = DownloadSpec::from(model);
        if let Some(url) = &self.url_override {
            spec.url.clone_from(url);
        }
        spec
    }

    /// Downloads and verifies `model`. Resumes an earlier partial download. Returns the path of
    /// the verified file. Cancelling keeps the partial file for later.
    pub async fn install<F>(
        &self,
        model: &'static ModelEntry,
        mut on_progress: F,
        cancel: &CancellationToken,
    ) -> Result<PathBuf>
    where
        F: FnMut(Progress) + Send,
    {
        if !model.is_verified() {
            return Err(Error::UnverifiedModel(model.id.to_owned()));
        }
        let guard = self.begin(
            model,
            Transient::Downloading(Progress {
                downloaded: 0,
                total: model.size,
                bytes_per_second: 0.0,
            }),
        )?;
        let path = self.model_path(model);
        let state = &self.state;
        let result = self
            .downloader
            .download(
                &self.spec(model),
                &path,
                |p| {
                    lock(state)
                        .transient
                        .insert(model.id, Transient::Downloading(p));
                    on_progress(p);
                },
                cancel,
            )
            .await;
        match result {
            Ok(()) => {
                guard.finish(None);
                Ok(path)
            }
            Err(Error::Cancelled) => {
                guard.finish(None);
                Err(Error::Cancelled)
            }
            Err(e) => {
                guard.finish(Some(Transient::Failed(e.to_string())));
                Err(e)
            }
        }
    }

    /// Re-hashes the installed file. `Ok(false)` if it is missing or corrupt.
    pub async fn verify(&self, model: &ModelEntry, cancel: &CancellationToken) -> Result<bool> {
        let path = self.model_path(model);
        if !path.is_file() {
            return Ok(false);
        }
        verify_file(&path, &DownloadSpec::from(model), cancel).await
    }

    /// Deletes the model file and any partial download. Unloads it first if loaded.
    pub fn remove(&self, model: &'static ModelEntry) -> Result<()> {
        let guard = self.begin(model, Transient::Loading)?;
        let unloaded = {
            let mut state = lock(&self.state);
            if state.engine.as_ref().is_some_and(|(id, _)| *id == model.id) {
                state.engine.take()
            } else {
                None
            }
        };
        // Dropping the engine stops its thread once in-flight users release their handles.
        drop(unloaded);
        let path = self.model_path(model);
        let result = remove_if_exists(&path).and_then(|()| remove_if_exists(&part_path(&path)));
        guard.finish(None);
        result.map_err(Error::from)
    }

    /// Loads `model` (which must be installed) and makes it the active engine.
    pub async fn load(
        &self,
        model: &'static ModelEntry,
        options: EngineOptions,
    ) -> Result<Arc<Engine>> {
        if let Some((id, engine)) = &lock(&self.state).engine
            && *id == model.id
        {
            return Ok(engine.clone());
        }
        let Some((path, _)) = self.installed(model) else {
            return Err(Error::NotInstalled(model.id.to_owned()));
        };
        let guard = self.begin(model, Transient::Loading)?;
        match Engine::load(path, options).await {
            Ok(engine) => {
                let engine = Arc::new(engine);
                let previous = lock(&self.state).engine.replace((model.id, engine.clone()));
                drop(previous);
                guard.finish(None);
                Ok(engine)
            }
            Err(e) => {
                guard.finish(Some(Transient::Failed(e.to_string())));
                Err(e)
            }
        }
    }

    /// The active engine, if any.
    pub fn engine(&self) -> Option<Arc<Engine>> {
        lock(&self.state).engine.as_ref().map(|(_, e)| e.clone())
    }

    /// Releases the active engine (memory is freed once all handles are dropped).
    pub fn unload(&self) {
        let engine = lock(&self.state).engine.take();
        drop(engine);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Tier;
    use crate::download::UrlPolicy;
    use sha2::{Digest, Sha256};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    fn fake_model(data: &[u8]) -> &'static ModelEntry {
        Box::leak(Box::new(ModelEntry {
            id: leak(format!("test-{}", data.len())),
            display_name: "Test",
            repo: "test/test",
            revision: "0123456789abcdef0123456789abcdef01234567",
            file_name: leak(format!("test-{}.gguf", data.len())),
            size: data.len() as u64,
            sha256: leak(hex::encode(Sha256::digest(data))),
            license: "MIT",
            license_url: "https://huggingface.co/test",
            model_card_url: "https://huggingface.co/test",
            tier: Tier::Desktop,
            approx_ram: 0,
        }))
    }

    async fn setup(data: &[u8]) -> (MockServer, tempfile::TempDir, ModelManager) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.to_vec()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let mut manager = ModelManager::with_downloader(
            dir.path(),
            Downloader::with_policy(UrlPolicy::loopback_for_tests()).expect("client"),
        );
        manager.url_override = Some(format!("{}/model.gguf", server.uri()));
        (server, dir, manager)
    }

    #[tokio::test]
    async fn install_status_remove() {
        let data = vec![7u8; 4096];
        let model = fake_model(&data);
        let (_server, dir, manager) = setup(&data).await;
        assert_eq!(
            manager.models_dir(),
            dir.path().join("local-ai").join("models")
        );
        assert_eq!(manager.status(model), ModelStatus::NotInstalled);

        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let path = manager
            .install(
                model,
                move |p| sink.lock().expect("lock").push(p),
                &CancellationToken::new(),
            )
            .await
            .expect("install");
        assert_eq!(path, manager.model_path(model));
        assert_eq!(
            manager.status(model),
            ModelStatus::Installed {
                path: path.clone(),
                size: 4096
            }
        );
        assert!(!seen.lock().expect("lock").is_empty());
        assert!(
            manager
                .verify(model, &CancellationToken::new())
                .await
                .expect("verify")
        );

        manager.remove(model).expect("remove");
        assert!(!path.exists());
        assert_eq!(manager.status(model), ModelStatus::NotInstalled);
        // Removing again is fine.
        manager.remove(model).expect("idempotent");
    }

    #[tokio::test]
    async fn failed_install_reports_error_and_can_be_retried() {
        let data = vec![1u8; 1000];
        let model = fake_model(&vec![2u8; 1000]);
        let (_server, _dir, manager) = setup(&data).await;
        let err = manager
            .install(model, |_| {}, &CancellationToken::new())
            .await
            .expect_err("hash mismatch");
        assert!(matches!(err, Error::HashMismatch { .. }));
        assert!(matches!(manager.status(model), ModelStatus::Error { .. }));
        // An error does not block a new attempt.
        let err = manager
            .install(model, |_| {}, &CancellationToken::new())
            .await
            .expect_err("still mismatching");
        assert!(matches!(err, Error::HashMismatch { .. }));
    }

    #[tokio::test]
    async fn concurrent_operations_are_rejected() {
        let data = vec![3u8; 100];
        let model = fake_model(&data);
        let (_server, _dir, manager) = setup(&data).await;
        let guard = manager.begin(model, Transient::Loading).expect("first");
        assert!(matches!(
            manager
                .install(model, |_| {}, &CancellationToken::new())
                .await,
            Err(Error::Busy(_))
        ));
        assert!(matches!(manager.remove(model), Err(Error::Busy(_))));
        assert_eq!(manager.status(model), ModelStatus::Loading);
        drop(guard);
        assert_eq!(manager.status(model), ModelStatus::NotInstalled);
    }

    #[tokio::test]
    async fn cancelled_install_returns_to_not_installed() {
        let data = vec![4u8; 100];
        let model = fake_model(&data);
        let (_server, _dir, manager) = setup(&data).await;
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(matches!(
            manager.install(model, |_| {}, &cancel).await,
            Err(Error::Cancelled)
        ));
        assert_eq!(manager.status(model), ModelStatus::NotInstalled);
    }

    #[tokio::test]
    async fn load_requires_installation() {
        let data = vec![5u8; 100];
        let model = fake_model(&data);
        let (_server, _dir, manager) = setup(&data).await;
        assert!(matches!(
            manager.load(model, EngineOptions::default()).await,
            Err(Error::NotInstalled(_))
        ));
        assert!(manager.engine().is_none());
    }

    #[tokio::test]
    async fn load_of_invalid_model_reports_error() {
        let data = b"not a gguf".to_vec();
        let model = fake_model(&data);
        let (_server, _dir, manager) = setup(&data).await;
        manager
            .install(model, |_| {}, &CancellationToken::new())
            .await
            .expect("install");
        let err = manager
            .load(model, EngineOptions::default())
            .await
            .expect_err("cannot load");
        if cfg!(feature = "llama") {
            assert!(matches!(err, Error::ModelLoad { .. }), "{err:?}");
        } else {
            assert!(matches!(err, Error::Unavailable), "{err:?}");
        }
        assert!(matches!(manager.status(model), ModelStatus::Error { .. }));
        manager.unload();
    }

    #[test]
    fn unverified_models_are_refused() {
        let model: &'static ModelEntry = Box::leak(Box::new(ModelEntry {
            sha256: "TODO(verify)",
            ..catalog::QWEN25_CODER_0_5B_Q4_K_M
        }));
        let dir = tempfile::tempdir().expect("tmp");
        let manager = ModelManager::new(dir.path()).expect("manager");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("rt");
        let err = rt
            .block_on(manager.install(model, |_| {}, &CancellationToken::new()))
            .expect_err("fail closed");
        assert!(matches!(err, Error::UnverifiedModel(_)));
    }

    #[test]
    fn status_serializes_with_tag() {
        let s = serde_json::to_value(ModelStatus::Installed {
            path: PathBuf::from("/m"),
            size: 1,
        })
        .expect("json");
        assert_eq!(s["state"], "installed");
        assert_eq!(s["size"], 1);
        let s = serde_json::to_value(ModelStatus::NotInstalled).expect("json");
        assert_eq!(s["state"], "not_installed");
    }

    #[test]
    fn default_model_is_platform_default() {
        assert_eq!(ModelManager::default_model(), catalog::default_model());
    }
}
