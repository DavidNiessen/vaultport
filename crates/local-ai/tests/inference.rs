// SPDX-License-Identifier: GPL-3.0-or-later
//! End-to-end inference against a real model file.
//!
//! These tests are ignored by default because they need a ~500 MB model. Run them with
//!
//! ```sh
//! LOCAL_AI_TEST_MODEL=/path/to/qwen2.5-coder-0.5b-instruct-q4_k_m.gguf \
//!     cargo test -p local-ai --release --test inference -- --ignored --nocapture
//! ```
//!
//! The file is verified against the pinned catalog digest first, so the test also proves that
//! the catalog entry matches the published file.

#![cfg(feature = "llama")]
#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::time::Instant;

use local_ai::catalog::QWEN25_CODER_0_5B_Q4_K_M;
use local_ai::{
    CancellationToken, CommandContext, Engine, EngineOptions, Error, GenerateRequest, ModelManager,
    SnippetRequest,
};

fn model_path() -> PathBuf {
    PathBuf::from(
        std::env::var_os("LOCAL_AI_TEST_MODEL")
            .expect("set LOCAL_AI_TEST_MODEL to the 0.5B Q4_K_M GGUF file"),
    )
}

fn context() -> CommandContext {
    CommandContext {
        os: Some("Ubuntu 24.04".into()),
        shell: Some("bash".into()),
        cwd: Some("/home/alice/projects".into()),
        recent_commands: vec!["cd projects".into(), "git status".into()],
        selection: None,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a real model file (LOCAL_AI_TEST_MODEL)"]
async fn pipeline_with_real_model() {
    let path = model_path();

    // The pinned catalog values must match the real file.
    let dir = tempfile::tempdir().expect("tmp");
    let manager = ModelManager::new(dir.path()).expect("manager");
    std::fs::create_dir_all(manager.models_dir()).expect("mkdir");
    let installed = manager.model_path(&QWEN25_CODER_0_5B_Q4_K_M);
    std::fs::hard_link(&path, &installed)
        .or_else(|_| std::fs::copy(&path, &installed).map(|_| ()))
        .expect("stage model");
    assert!(
        manager
            .verify(&QWEN25_CODER_0_5B_Q4_K_M, &CancellationToken::new())
            .await
            .expect("hash"),
        "file does not match the pinned SHA-256"
    );

    let started = Instant::now();
    let engine = manager
        .load(&QWEN25_CODER_0_5B_Q4_K_M, EngineOptions::default())
        .await
        .expect("load");
    println!("load: {:?}", started.elapsed());

    let cancel = CancellationToken::new();
    for prompt in [
        "list files sorted by size",
        "show disk usage of each directory here, human readable",
        "find all python files modified in the last 2 days",
        "delete all log files",
    ] {
        let started = Instant::now();
        let s = engine
            .generate(
                &GenerateRequest {
                    prompt: prompt.into(),
                    context: context(),
                },
                &cancel,
            )
            .await
            .expect("generate");
        println!(
            "{prompt:?}\n  -> {:?}\n     {:?} ({:?})",
            s.command,
            s.explanation,
            started.elapsed()
        );
        assert!(!s.command.is_empty());
        assert!(!s.command.contains('\n'));
    }

    let s = engine
        .generate(
            &GenerateRequest {
                prompt: "list files sorted by size".into(),
                context: context(),
            },
            &cancel,
        )
        .await
        .expect("generate");
    assert!(
        s.command.starts_with("ls") || s.command.starts_with("du") || s.command.contains("sort"),
        "unexpected command {:?}",
        s.command
    );

    let started = Instant::now();
    let snippet = engine
        .generate_snippet(
            &SnippetRequest {
                description: "compress every .log file in /var/log/myapp older than 7 days".into(),
                context: context(),
            },
            &cancel,
        )
        .await
        .expect("snippet");
    println!(
        "snippet ({:?}):\n{}\n  {:?}",
        started.elapsed(),
        snippet.script,
        snippet.explanation
    );
    assert!(!snippet.script.trim().is_empty());

    // Cancellation is honoured.
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let err = engine
        .generate(
            &GenerateRequest {
                prompt: "show uptime".into(),
                context: CommandContext::default(),
            },
            &cancelled,
        )
        .await
        .expect_err("cancelled");
    assert!(matches!(err, Error::Cancelled), "{err:?}");

    // Timeout is honoured.
    let quick = Engine::load(
        &path,
        EngineOptions {
            timeout: std::time::Duration::from_millis(1),
            ..EngineOptions::default()
        },
    )
    .await
    .expect("load");
    let err = quick
        .generate(
            &GenerateRequest {
                prompt: "show uptime".into(),
                context: CommandContext::default(),
            },
            &cancel,
        )
        .await
        .expect_err("timeout");
    assert!(matches!(err, Error::Timeout), "{err:?}");
}
