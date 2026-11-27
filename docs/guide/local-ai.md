---
title: Local AI
description: Optional, fully on-device command generation with a small pinned model and llama.cpp. Off by default; nothing you type leaves your device and nothing runs without you.
---

# Local AI

ssh-client can turn a plain-language request such as _"find the 10 largest files in /var/log"_ into a shell command — using a small language model that runs **entirely on your device**. It is **off by default** and completely optional; every other feature works without it.

## Turning it on

1. **Settings → Local AI → Enable local command generation.**
2. **Download model.** The model is downloaded once; afterwards everything works offline.

| Device             | Model                                | Download size | License    |
| ------------------ | ------------------------------------ | ------------- | ---------- |
| Desktop            | Qwen2.5-Coder 1.5B Instruct (Q4_K_M) | ≈ 1.1 GB      | Apache-2.0 |
| Phones and tablets | Qwen2.5-Coder 0.5B Instruct (Q4_K_M) | ≈ 490 MB      | Apache-2.0 |

**Remove model** deletes every file the feature created.

## Using it

Open the terminal side panel (<kbd>Mod</kbd>+<kbd>.</kbd>) and choose **AI**:

1. **Describe what you want to do.** Optionally **use selected terminal text as context**, for example an error message.
2. **Generate command.** You get one command with a short explanation and a **risk rating** (low, medium, high, critical) from ssh-client's deterministic risk checker.
3. **Insert into terminal.** The command is typed at the prompt — never executed. Review it and press <kbd>Enter</kbd> yourself.

The engine can take more session context into account (remote OS, shell, working directory, recent commands); the current app passes only the request and, if you choose, the selected terminal text.

In the Snippets editor, **Generate script from description** drafts a multi-line snippet the same way.

::: tip Not autocomplete
AI is never used for autocomplete. Completion is a separate, deterministic engine — see [Snippets & autocomplete](/guide/snippets#autocomplete-deterministic-no-ai).
:::

## Privacy

- **No data leaves the device for inference.** Prompts, terminal context and generated commands stay in memory; they are not logged or stored.
- The **only network traffic** is the one-time download you start: anonymous HTTPS requests to Hugging Face and its CDN, with a generic user agent, no cookies, no tokens and no telemetry.

## Security

- Each model file is pinned to an immutable Hugging Face commit, with its exact size and SHA-256; a file that does not match is rejected. Downloads are HTTPS-only to an allow-list of hosts, re-checked on every redirect, and can be resumed.
- Model output is constrained by a grammar to a fixed JSON shape and validated: control characters, NUL bytes, bidirectional-override characters, multiple lines in single-command mode and oversized output are rejected.
- Text that comes from the remote host is treated as untrusted data — sanitized, size-capped and clearly framed as data in the prompt — to blunt prompt-injection attempts.

## Performance

Inference uses [llama.cpp](https://github.com/ggml-org/llama.cpp) on the CPU; Apple silicon builds can offload to the GPU through Metal. Speed depends on your hardware, which is why phones and tablets use the smaller model.
