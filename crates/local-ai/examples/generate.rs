// SPDX-License-Identifier: GPL-3.0-or-later
//! Developer tool: generate a command with a local GGUF model.
//!
//! ```sh
//! cargo run -p local-ai --release --example generate -- model.gguf "list files sorted by size"
//! ```
//!
//! Optional environment: `LOCAL_AI_THREADS`, `LOCAL_AI_GPU_LAYERS`, `LOCAL_AI_SNIPPET=1`.

#![allow(clippy::print_stdout)]

use std::time::Instant;

use local_ai::{
    CancellationToken, CommandContext, Engine, EngineOptions, GenerateRequest, SnippetRequest,
};

fn env_u32(name: &str) -> Option<u32> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(model), Some(prompt)) = (args.next(), args.next()) else {
        return Err("usage: generate <model.gguf> <request>".into());
    };
    let options = EngineOptions {
        threads: env_u32("LOCAL_AI_THREADS"),
        gpu_layers: env_u32("LOCAL_AI_GPU_LAYERS"),
        ..EngineOptions::default()
    };
    println!("threads: {}", options.effective_threads());
    let started = Instant::now();
    let engine = Engine::load(model, options).await?;
    println!("loaded in {:?}", started.elapsed());

    let context = CommandContext {
        os: Some(std::env::consts::OS.into()),
        shell: std::env::var("SHELL").ok(),
        cwd: std::env::current_dir()
            .ok()
            .map(|p| p.display().to_string()),
        ..CommandContext::default()
    };
    let cancel = CancellationToken::new();
    let started = Instant::now();
    if std::env::var_os("LOCAL_AI_SNIPPET").is_some() {
        let s = engine
            .generate_snippet(
                &SnippetRequest {
                    description: prompt,
                    context,
                },
                &cancel,
            )
            .await?;
        println!("{}\n# {}", s.script, s.explanation);
    } else {
        let s = engine
            .generate(&GenerateRequest { prompt, context }, &cancel)
            .await?;
        println!("{}\n# {}", s.command, s.explanation);
    }
    println!("generated in {:?}", started.elapsed());
    Ok(())
}
