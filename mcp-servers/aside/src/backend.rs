//! Backend CLI adapters.
//!
//! Each backend has one function that:
//!  1. Composes the prompt (context / transcript / question).
//!  2. Spawns the CLI via `tokio::process::Command` (kill_on_drop).
//!  3. Captures stdout as the backend's reply.
//!
//! There is intentionally no wall-clock timeout — advisor calls can
//! legitimately take minutes on complex prompts (the built-in `advisor()`
//! has no timeout either). The caller interrupts if they want to abort.
//!
//! Exact invocation flags confirmed from the user's local `--help` output at
//! plan time (2026-04-14). The argv template per backend is localised in
//! `build_command` so future CLI syntax drift is a single-line change.

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

const MAX_CAPTURED_STDERR: usize = 2 * 1024;
const MAX_CAPTURED_STDOUT: usize = 50 * 1024;

/// Which CLI we're talking to. Each variant maps to a concrete command builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Codex,
    Gemini,
    Copilot,
}

impl Backend {
    pub fn binary(&self) -> &'static str {
        match self {
            Backend::Codex => "codex",
            Backend::Gemini => "gemini",
            Backend::Copilot => "copilot",
        }
    }
}

/// Structured outcome of a backend call. Returned to the tool layer which
/// converts it into `CallToolResult`.
pub enum InvokeOutcome {
    Ok { stdout: String, truncated: bool, note: Option<String> },
    NotFound { binary: &'static str, hint: String },
    Failed { code: Option<i32>, stderr: String },
    Spawn(String),
    /// Client cancelled the request (MCP CancelledNotification). The child
    /// process was killed as part of the select arm's future drop path
    /// (kill_on_drop=true on the Command).
    Cancelled,
}

/// Ask a backend a question.
///
/// There is intentionally no wall-clock timeout — advisor-style CLIs can
/// legitimately take minutes, and the built-in `advisor()` has no timeout
/// either. Cancellation is driven by the MCP request's `CancellationToken`:
/// when the client sends a CancelledNotification (or Claude Code tears down
/// the session) rmcp calls `ct.cancel()`, this function's `tokio::select!`
/// abandons the `wait_with_output` future, the child is dropped, and
/// `kill_on_drop(true)` sends SIGKILL. The subprocess does NOT outlive a
/// cancelled request.
///
/// The only case not covered is "client stays connected, child wedges,
/// no cancellation" — the user walked away. That's by design.
pub async fn invoke(
    backend: Backend,
    prompt: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
    ct: &CancellationToken,
) -> InvokeOutcome {
    if which(backend.binary()).is_none() {
        return InvokeOutcome::NotFound {
            binary: backend.binary(),
            hint: install_hint(backend),
        };
    }

    let mut cmd = build_command(backend, prompt, model, reasoning_effort);
    let pipe_prompt_on_stdin = matches!(backend, Backend::Gemini);

    if pipe_prompt_on_stdin {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return InvokeOutcome::Spawn(format!("spawn {} failed: {}", backend.binary(), e));
        }
    };

    // gemini: argv already carries the prompt, but the CLI also accepts stdin
    // and appends it to the argv prompt. We pipe the prompt again on stdin —
    // harmless duplication but ensures long transcripts get through even if
    // they strain argv limits.
    if pipe_prompt_on_stdin {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(prompt.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
    }

    let output = tokio::select! {
        biased;
        _ = ct.cancelled() => {
            // child is owned by the wait future; dropping this select arm
            // drops the future, dropping the child, triggering kill_on_drop.
            return InvokeOutcome::Cancelled;
        }
        res = child.wait_with_output() => match res {
            Ok(o) => o,
            Err(e) => return InvokeOutcome::Spawn(format!("wait failed: {}", e)),
        }
    };

    if !output.status.success() {
        let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if stderr.len() > MAX_CAPTURED_STDERR {
            let keep = &stderr[stderr.len() - MAX_CAPTURED_STDERR..];
            stderr = format!(
                "[stderr truncated to last {} bytes]\n{}",
                MAX_CAPTURED_STDERR, keep
            );
        }
        return InvokeOutcome::Failed { code: output.status.code(), stderr };
    }

    let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let mut truncated = false;
    if stdout.len() > MAX_CAPTURED_STDOUT {
        let keep: String = stdout.chars().take(MAX_CAPTURED_STDOUT).collect();
        let orig_len = stdout.len();
        stdout = format!(
            "{}\n\n[response truncated after {} bytes; original was {} bytes]",
            keep, MAX_CAPTURED_STDOUT, orig_len
        );
        truncated = true;
    }

    let note = if matches!(backend, Backend::Gemini) && reasoning_effort.is_some() {
        Some(
            "note: gemini CLI currently exposes no reasoning-effort flag; the value was ignored"
                .to_string(),
        )
    } else {
        None
    };

    InvokeOutcome::Ok { stdout, truncated, note }
}

/// Build the `Command` for a backend. Prompt and flags inlined; stdio is
/// configured by the caller in `invoke`.
fn build_command(
    backend: Backend,
    prompt: &str,
    model: Option<&str>,
    reasoning_effort: Option<&str>,
) -> Command {
    match backend {
        Backend::Codex => {
            // codex -s read-only -a never [-m MODEL] [-c model_reasoning_effort=EFF] exec "<PROMPT>"
            //   -s read-only: sandbox blocks file writes / shell side effects but ALLOWS reads,
            //                 so codex can open files the caller references by path.
            //   -a never:     skip approval prompts (non-interactive)
            //   -c ...:       TOML config override for reasoning effort
            //   exec:         non-interactive subcommand; prompt is the positional arg
            let mut cmd = Command::new("codex");
            cmd.arg("-s").arg("read-only");
            cmd.arg("-a").arg("never");
            if let Some(m) = model {
                cmd.arg("-m").arg(m);
            }
            if let Some(eff) = reasoning_effort {
                cmd.arg("-c").arg(format!("model_reasoning_effort={}", eff));
            }
            cmd.arg("exec");
            cmd.arg(prompt);
            cmd
        }
        Backend::Gemini => {
            // gemini -p "<PROMPT>" --approval-mode plan -o text [-m MODEL]
            //   -p:                  non-interactive with prompt; appends stdin
            //   --approval-mode plan: plan mode — read / grep / web tools remain available to the
            //                         model, but NO edits, NO shell exec, NO approval prompts.
            //                         File reads are restricted to the spawn cwd workspace.
            //   -o text:             plain text output
            let mut cmd = Command::new("gemini");
            cmd.arg("-p").arg(prompt);
            cmd.arg("--approval-mode").arg("plan");
            cmd.arg("-o").arg("text");
            if let Some(m) = model {
                cmd.arg("-m").arg(m);
            }
            // reasoning_effort intentionally ignored; gemini CLI has no flag.
            cmd
        }
        Backend::Copilot => {
            // copilot -p "<PROMPT>" --allow-all-tools --available-tools=view,rg,glob,web_fetch
            //         -s --no-color [--model MODEL] [--effort EFF]
            //   -p:                  non-interactive prompt via argv
            //   --allow-all-tools:   required for non-interactive mode per help (auto-approve
            //                        whatever is in --available-tools; no approval prompts)
            //   --available-tools=…: read-only tool whitelist so copilot can inspect files the
            //                        caller references by path:
            //                          view      — read file contents
            //                          rg        — ripgrep across the workspace
            //                          glob      — file path pattern match
            //                          web_fetch — fetch URL bodies for docs / spec lookups
            //                        Intentionally excludes bash / write_bash / read_bash / task
            //                        / skill / sql / store_memory / report_intent, which would
            //                        let copilot exec shells or mutate state — aside is Q&A only.
            //   -s:                  silent (stdout contains only the response)
            //   --no-color:          strip ANSI for clean capture
            let mut cmd = Command::new("copilot");
            cmd.arg("-p").arg(prompt);
            cmd.arg("--allow-all-tools");
            cmd.arg("--available-tools=view,rg,glob,web_fetch");
            cmd.arg("-s");
            cmd.arg("--no-color");
            if let Some(m) = model {
                cmd.arg("--model").arg(m);
            }
            if let Some(eff) = reasoning_effort {
                cmd.arg("--effort").arg(eff);
            }
            cmd
        }
    }
}

fn install_hint(backend: Backend) -> String {
    match backend {
        Backend::Codex => {
            "install codex CLI (`npm i -g @openai/codex`; see https://github.com/openai/codex)"
                .to_string()
        }
        Backend::Gemini => {
            "install gemini CLI (`npm i -g @google/gemini-cli`; see https://github.com/google-gemini/gemini-cli)"
                .to_string()
        }
        Backend::Copilot => {
            "install copilot CLI (see https://docs.github.com/copilot/how-tos/copilot-cli)"
                .to_string()
        }
    }
}

/// Minimal PATH lookup — returns Some(path) if the binary is executable.
pub fn which(binary: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(binary);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{}.exe", binary));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Ask the backend CLI for its `--version` string. Returns `None` if missing.
pub async fn version(backend: Backend) -> Option<String> {
    let _ = which(backend.binary())?;
    let output = Command::new(backend.binary())
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;
    let out = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !out.is_empty() {
        return Some(out);
    }
    let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if err.is_empty() { None } else { Some(err) }
}
