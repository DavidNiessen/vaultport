// SPDX-License-Identifier: GPL-3.0-or-later
//! On-device inference.
//!
//! With the `llama` cargo feature (default) the [`Engine`] runs a GGUF model through llama.cpp
//! on a **dedicated OS thread**: the async API never blocks the Tokio runtime, requests are
//! queued and processed one at a time, and dropping the [`Engine`] stops the thread and frees
//! the model. Output is constrained by a GBNF grammar ([`crate::prompt::COMMAND_GRAMMAR`]),
//! sampled at a low temperature, bounded in tokens and wall-clock time, cancellable, and finally
//! validated by [`crate::prompt`] before anything is returned.
//!
//! Without the feature the same API exists but [`Engine::load`] returns
//! [`Error::Unavailable`], so distributions can ship without any AI code.
//!
//! GPU offload: Metal is used on Apple Silicon (feature `metal`, enabled automatically by
//! llama.cpp's build on aarch64 macOS), Vulkan with feature `vulkan`, otherwise the CPU.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::error::{Error, Result};
use crate::prompt::{GenerateRequest, Snippet, SnippetRequest, Suggestion};

/// Tuning knobs for loading and running a model.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineOptions {
    /// CPU threads for inference. `None`: all logical CPUs up to 4, otherwise half of them
    /// (approximating physical / performance cores), at most 8.
    pub threads: Option<u32>,
    /// Layers to offload to the GPU. `None`: llama.cpp's default (all layers when a GPU backend
    /// is compiled in and available, otherwise none). `Some(0)` forces CPU inference.
    pub gpu_layers: Option<u32>,
    /// Context window in tokens (prompt plus output).
    pub ctx_size: u32,
    /// Maximum generated tokens for a single command.
    pub max_tokens: u32,
    /// Maximum generated tokens for a snippet.
    pub snippet_max_tokens: u32,
    /// Sampling temperature. Low values make the output nearly deterministic.
    pub temperature: f32,
    /// Wall-clock limit per request, including time spent waiting in the queue.
    pub timeout: Duration,
    /// Sampling seed.
    pub seed: u32,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            threads: None,
            gpu_layers: None,
            ctx_size: 2048,
            max_tokens: 256,
            snippet_max_tokens: 768,
            temperature: 0.2,
            timeout: Duration::from_secs(60),
            seed: 0x5eed,
        }
    }
}

impl EngineOptions {
    /// The thread count that will be used.
    pub fn effective_threads(&self) -> u32 {
        self.threads.filter(|t| *t > 0).unwrap_or_else(|| {
            let logical = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
            let threads = if logical <= 4 {
                logical
            } else {
                (logical / 2).clamp(4, 8)
            };
            threads.max(1) as u32
        })
    }
}

/// Whether this build contains the inference backend.
pub const fn is_available() -> bool {
    cfg!(feature = "llama")
}

#[cfg(feature = "llama")]
pub use llama::Engine;

#[cfg(not(feature = "llama"))]
pub use stub::Engine;

#[cfg(not(feature = "llama"))]
mod stub {
    use super::{
        CancellationToken, EngineOptions, Error, GenerateRequest, Path, PathBuf, Result, Snippet,
        SnippetRequest, Suggestion,
    };

    /// A loaded model (unavailable in this build: the `llama` feature is disabled).
    #[derive(Debug)]
    pub struct Engine {
        never: std::convert::Infallible,
    }

    #[allow(clippy::unused_async)]
    impl Engine {
        /// Always fails with [`Error::Unavailable`] in this build.
        pub async fn load(path: impl Into<PathBuf>, options: EngineOptions) -> Result<Self> {
            let _ = (path.into(), options);
            Err(Error::Unavailable)
        }

        /// The model file.
        pub fn path(&self) -> &Path {
            match self.never {}
        }

        /// The options the engine was loaded with.
        pub fn options(&self) -> &EngineOptions {
            match self.never {}
        }

        /// Generates one shell command.
        pub async fn generate(
            &self,
            _request: &GenerateRequest,
            _cancel: &CancellationToken,
        ) -> Result<Suggestion> {
            match self.never {}
        }

        /// Generates a multi-line script.
        pub async fn generate_snippet(
            &self,
            _request: &SnippetRequest,
            _cancel: &CancellationToken,
        ) -> Result<Snippet> {
            match self.never {}
        }
    }
}

#[cfg(feature = "llama")]
mod llama {
    use std::num::NonZeroU32;
    use std::sync::{OnceLock, mpsc};
    use std::time::Instant;

    use llama_cpp_2::context::LlamaContext;
    use llama_cpp_2::context::params::LlamaContextParams;
    use llama_cpp_2::llama_backend::LlamaBackend;
    use llama_cpp_2::llama_batch::LlamaBatch;
    use llama_cpp_2::model::params::LlamaModelParams;
    use llama_cpp_2::model::{LlamaChatMessage, LlamaChatTemplate, LlamaModel};
    use llama_cpp_2::sampling::LlamaSampler;
    use llama_cpp_2::token::LlamaToken;
    use llama_cpp_2::token::data::LlamaTokenData;
    use llama_cpp_2::token::data_array::LlamaTokenDataArray;
    use tokio::sync::oneshot;

    use super::{
        CancellationToken, EngineOptions, Error, GenerateRequest, Path, PathBuf, Result, Snippet,
        SnippetRequest, Suggestion,
    };
    use crate::prompt::{self, ChatMessage, Mode, STOP_SEQUENCES};

    /// llama.cpp may be initialized only once per process; the backend lives forever.
    static BACKEND: OnceLock<std::result::Result<LlamaBackend, String>> = OnceLock::new();

    fn backend() -> Result<&'static LlamaBackend> {
        BACKEND
            .get_or_init(|| {
                // Route llama.cpp's own logging through `tracing` instead of stderr.
                llama_cpp_2::send_logs_to_tracing(llama_cpp_2::LogOptions::default());
                LlamaBackend::init().map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| Error::Inference(format!("backend initialization failed: {e}")))
    }

    struct Job {
        mode: Mode,
        messages: Vec<ChatMessage>,
        cancel: CancellationToken,
        deadline: Instant,
        reply: oneshot::Sender<Result<String>>,
    }

    /// A loaded model with its own inference thread.
    #[derive(Debug)]
    pub struct Engine {
        jobs: mpsc::Sender<Job>,
        path: PathBuf,
        options: EngineOptions,
    }

    impl Engine {
        /// Loads a GGUF model on a new inference thread. Resolves once the model is in memory
        /// and a context has been created.
        pub async fn load(path: impl Into<PathBuf>, options: EngineOptions) -> Result<Self> {
            let path = path.into();
            if !path.is_file() {
                return Err(Error::ModelLoad {
                    path,
                    message: "file not found".into(),
                });
            }
            if options.ctx_size < 256 {
                return Err(Error::InvalidRequest("ctx_size must be at least 256"));
            }
            let (ready_tx, ready_rx) = oneshot::channel();
            let (jobs, job_rx) = mpsc::channel::<Job>();
            let thread_path = path.clone();
            let thread_options = options.clone();
            std::thread::Builder::new()
                .name("local-ai-inference".into())
                .spawn(move || worker(&thread_path, &thread_options, ready_tx, &job_rx))?;
            ready_rx
                .await
                .map_err(|_| Error::Inference("inference thread exited while loading".into()))??;
            Ok(Self {
                jobs,
                path,
                options,
            })
        }

        /// The model file.
        pub fn path(&self) -> &Path {
            &self.path
        }

        /// The options the engine was loaded with.
        pub fn options(&self) -> &EngineOptions {
            &self.options
        }

        /// Generates one shell command (possibly a pipeline) for `request`.
        pub async fn generate(
            &self,
            request: &GenerateRequest,
            cancel: &CancellationToken,
        ) -> Result<Suggestion> {
            let messages = prompt::command_messages(request)?;
            let raw = self.submit(Mode::Command, messages, cancel).await?;
            prompt::parse_command_output(&raw)
        }

        /// Generates a multi-line script for `request`.
        pub async fn generate_snippet(
            &self,
            request: &SnippetRequest,
            cancel: &CancellationToken,
        ) -> Result<Snippet> {
            let messages = prompt::snippet_messages(request)?;
            let raw = self.submit(Mode::Snippet, messages, cancel).await?;
            prompt::parse_snippet_output(&raw)
        }

        async fn submit(
            &self,
            mode: Mode,
            messages: Vec<ChatMessage>,
            cancel: &CancellationToken,
        ) -> Result<String> {
            let (reply, rx) = oneshot::channel();
            let job = Job {
                mode,
                messages,
                cancel: cancel.clone(),
                deadline: Instant::now() + self.options.timeout,
                reply,
            };
            self.jobs
                .send(job)
                .map_err(|_| Error::Inference("inference thread is not running".into()))?;
            // Dropping this future closes `rx`, which the worker notices and stops early.
            rx.await
                .map_err(|_| Error::Inference("inference thread stopped".into()))?
        }
    }

    fn worker(
        path: &Path,
        options: &EngineOptions,
        ready: oneshot::Sender<Result<()>>,
        jobs: &mpsc::Receiver<Job>,
    ) {
        let load_err = |message: String| Error::ModelLoad {
            path: path.to_owned(),
            message,
        };
        let backend = match backend() {
            Ok(b) => b,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let mut model_params = LlamaModelParams::default();
        if let Some(layers) = options.gpu_layers {
            model_params = model_params.with_n_gpu_layers(layers);
        }
        let started = Instant::now();
        let model = match LlamaModel::load_from_file(backend, path, &model_params) {
            Ok(m) => m,
            Err(e) => {
                let _ = ready.send(Err(load_err(e.to_string())));
                return;
            }
        };
        let threads = options.effective_threads() as i32;
        let ctx_size = options.ctx_size.min(model.n_ctx_train().max(256));
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(ctx_size))
            .with_n_batch(ctx_size)
            .with_n_threads(threads)
            .with_n_threads_batch(threads);
        let mut ctx = match model.new_context(backend, ctx_params) {
            Ok(c) => c,
            Err(e) => {
                let _ = ready.send(Err(load_err(format!("cannot create context: {e}"))));
                return;
            }
        };
        let template = model.chat_template(None).ok();
        tracing::info!(
            path = %path.display(),
            threads,
            ctx_size,
            params = model.n_params(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            embedded_template = template.is_some(),
            "local model loaded"
        );
        if ready.send(Ok(())).is_err() {
            return; // caller gave up while loading
        }
        // Tokens currently held in the KV cache. The system prompt and few-shot examples are the
        // same for every request, so their KV entries are reused instead of being recomputed.
        let mut cached: Vec<LlamaToken> = Vec::new();
        while let Ok(job) = jobs.recv() {
            if job.reply.is_closed() {
                continue;
            }
            let result = run(
                &model,
                &mut ctx,
                &mut cached,
                template.as_ref(),
                options,
                &job,
            );
            if result.is_err() {
                // The cache may be half-written; start from scratch next time.
                ctx.clear_kv_cache();
                cached.clear();
            }
            let _ = job.reply.send(result);
        }
        tracing::debug!("inference thread exiting");
    }

    fn check(job: &Job) -> Result<()> {
        if job.cancel.is_cancelled() || job.reply.is_closed() {
            Err(Error::Cancelled)
        } else if Instant::now() >= job.deadline {
            Err(Error::Timeout)
        } else {
            Ok(())
        }
    }

    fn render(
        model: &LlamaModel,
        template: Option<&LlamaChatTemplate>,
        messages: &[ChatMessage],
    ) -> String {
        if let Some(template) = template {
            let chat: std::result::Result<Vec<_>, _> = messages
                .iter()
                .map(|m| LlamaChatMessage::new(m.role.to_owned(), m.content.clone()))
                .collect();
            match chat.map(|chat| model.apply_chat_template(template, &chat, true)) {
                Ok(Ok(rendered)) => return rendered,
                Ok(Err(e)) => tracing::debug!(error = %e, "embedded chat template failed"),
                Err(e) => tracing::debug!(error = %e, "invalid chat message"),
            }
        }
        prompt::format_chatml(messages)
    }

    #[allow(clippy::needless_pass_by_value)]
    fn infer(what: &str, e: impl std::fmt::Display) -> Error {
        Error::Inference(format!("{what}: {e}"))
    }

    /// Samples one token that the grammar accepts.
    ///
    /// Applying a grammar to the whole vocabulary (~150k tokens for Qwen) at every step is slow,
    /// so — like llama.cpp's own `common_sampler` — the token is first sampled without the
    /// grammar and only checked against it. Only if the grammar rejects it is the full
    /// distribution constrained and sampled again. The result is identical to always applying
    /// the grammar first, because a grammar only ever removes candidates.
    fn sample(
        ctx: &LlamaContext<'_>,
        idx: i32,
        sampler: &mut LlamaSampler,
        grammar: &mut LlamaSampler,
    ) -> Result<LlamaToken> {
        let token = sampler.sample(ctx, idx);
        let mut single =
            LlamaTokenDataArray::new(vec![LlamaTokenData::new(token, 1.0, 0.0)], false);
        grammar.apply(&mut single);
        if single.data.first().is_some_and(|d| d.logit().is_finite()) {
            grammar.accept(token);
            return Ok(token);
        }
        let mut all = ctx.token_data_array_ith(idx);
        grammar.apply(&mut all);
        sampler.apply(&mut all);
        let token = all
            .selected_token()
            .ok_or_else(|| Error::InvalidOutput("grammar allows no continuation".into()))?;
        grammar.accept(token);
        Ok(token)
    }

    fn run(
        model: &LlamaModel,
        ctx: &mut LlamaContext<'_>,
        cached: &mut Vec<LlamaToken>,
        template: Option<&LlamaChatTemplate>,
        options: &EngineOptions,
        job: &Job,
    ) -> Result<String> {
        check(job)?;
        let started = Instant::now();

        let mut messages = job.messages.clone();
        prompt::strip_special_markers(&mut messages);
        let text = render(model, template, &messages);
        let vocab = model.vocab();
        let tokens = vocab.tokenize(text.as_bytes(), true, true);
        let max_new = match job.mode {
            Mode::Command => options.max_tokens,
            Mode::Snippet => options.snippet_max_tokens,
        } as usize;
        let n_ctx = ctx.n_ctx();
        if tokens.is_empty() || tokens.len() + max_new > n_ctx as usize {
            return Err(Error::PromptTooLong {
                tokens: tokens.len(),
                ctx_size: n_ctx,
            });
        }

        // Reuse the longest cached prefix; at least the last prompt token is always decoded so
        // that fresh logits exist.
        let mut reuse = cached
            .iter()
            .zip(&tokens)
            .take_while(|(a, b)| a == b)
            .count()
            .min(tokens.len() - 1);
        if !matches!(
            ctx.clear_kv_cache_seq(Some(0), Some(reuse as u32), None),
            Ok(true)
        ) {
            ctx.clear_kv_cache();
            reuse = 0;
        }
        cached.truncate(reuse);

        // Prompt processing, in batches.
        let n_batch = (ctx.n_batch() as usize).max(1);
        let mut batch = LlamaBatch::new(n_batch, 1);
        let last = tokens.len() - 1;
        for (chunk_index, chunk) in tokens[reuse..].chunks(n_batch).enumerate() {
            check(job)?;
            batch.clear();
            for (i, token) in chunk.iter().enumerate() {
                let pos = reuse + chunk_index * n_batch + i;
                batch
                    .add(*token, pos as i32, &[0], pos == last)
                    .map_err(|e| infer("batch", e))?;
            }
            ctx.decode(&mut batch)
                .map_err(|e| infer("prompt decode", e))?;
            cached.extend_from_slice(chunk);
        }
        let prompt_ms = started.elapsed().as_millis() as u64;

        let mut grammar = LlamaSampler::grammar(model, prompt::grammar(job.mode), "root")
            .map_err(|e| infer("grammar", e))?;
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::top_k(40),
            LlamaSampler::top_p(0.95, 1),
            LlamaSampler::temp(options.temperature.max(0.0)),
            LlamaSampler::dist(options.seed),
        ]);

        let mut out: Vec<u8> = Vec::with_capacity(512);
        for (generated, pos) in (0..max_new).zip(tokens.len()..) {
            check(job)?;
            let token = sample(ctx, batch.n_tokens() - 1, &mut sampler, &mut grammar)?;
            if vocab.is_eog(token) {
                tracing::debug!(
                    prompt_tokens = tokens.len(),
                    reused_tokens = reuse,
                    generated,
                    prompt_ms,
                    total_ms = started.elapsed().as_millis() as u64,
                    "generation finished"
                );
                return String::from_utf8(out)
                    .map_err(|_| Error::InvalidOutput("output is not valid UTF-8".into()));
            }
            out.extend_from_slice(&vocab.token_to_piece(token, false, None));
            if let Ok(s) = std::str::from_utf8(&out)
                && let Some(i) = STOP_SEQUENCES.iter().filter_map(|stop| s.find(stop)).min()
            {
                return Ok(s[..i].to_owned());
            }
            batch.clear();
            batch
                .add(token, pos as i32, &[0], true)
                .map_err(|e| infer("batch", e))?;
            ctx.decode(&mut batch).map_err(|e| infer("decode", e))?;
            cached.push(token);
        }
        Err(Error::InvalidOutput(format!(
            "no complete answer within {max_new} tokens"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options() {
        let o = EngineOptions::default();
        assert_eq!(o.max_tokens, 256);
        assert!((o.temperature - 0.2).abs() < f32::EPSILON);
        assert!(o.ctx_size >= 1024);
        let t = o.effective_threads();
        assert!((1..=8).contains(&t));
        let forced = EngineOptions {
            threads: Some(3),
            ..EngineOptions::default()
        };
        assert_eq!(forced.effective_threads(), 3);
    }

    #[test]
    fn availability_matches_feature() {
        assert_eq!(is_available(), cfg!(feature = "llama"));
    }

    #[tokio::test]
    async fn load_missing_file_fails() {
        let err = Engine::load("/nonexistent/model.gguf", EngineOptions::default())
            .await
            .expect_err("must fail");
        if cfg!(feature = "llama") {
            assert!(matches!(err, Error::ModelLoad { .. }), "{err:?}");
        } else {
            assert!(matches!(err, Error::Unavailable), "{err:?}");
        }
    }

    #[cfg(feature = "llama")]
    #[tokio::test]
    async fn load_garbage_file_fails_cleanly() {
        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("bad.gguf");
        std::fs::write(&path, b"definitely not a gguf file").expect("write");
        let err = Engine::load(&path, EngineOptions::default())
            .await
            .expect_err("must fail");
        assert!(matches!(err, Error::ModelLoad { .. }), "{err:?}");
    }
}
