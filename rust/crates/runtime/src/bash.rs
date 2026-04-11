use std::collections::HashMap;
use std::env;
use std::io;
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::process::Command as TokioCommand;
use tokio::runtime::Builder;

use crate::bash_validation::{self, ValidationResult};
use crate::permissions::PermissionMode;
use crate::sandbox::{
    build_linux_sandbox_command, resolve_sandbox_status_for_request, FilesystemIsolationMode,
    SandboxConfig, SandboxStatus,
};
use crate::ConfigLoader;

/// Global registry mapping background task IDs to their `JoinHandle`s.
/// Allows reaping of finished background processes and prevents orphaned threads.
static BACKGROUND_TASK_REGISTRY: LazyLock<Mutex<HashMap<String, JoinHandle<()>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, schemars::JsonSchema)]
pub struct BashCommandInput {
    pub command: String,
    pub timeout: Option<u64>,
    pub description: Option<String>,
    #[serde(rename = "run_in_background")]
    pub run_in_background: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "namespaceRestrictions")]
    pub namespace_restrictions: Option<bool>,
    #[serde(rename = "isolateNetwork")]
    pub isolate_network: Option<bool>,
    #[serde(rename = "filesystemMode")]
    pub filesystem_mode: Option<FilesystemIsolationMode>,
    #[serde(rename = "allowedMounts")]
    pub allowed_mounts: Option<Vec<String>>,
    #[serde(rename = "permissionMode")]
    pub permission_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BashCommandOutput {
    pub stdout: String,
    pub stderr: String,
    #[serde(rename = "rawOutputPath")]
    pub raw_output_path: Option<String>,
    pub interrupted: bool,
    #[serde(rename = "isImage")]
    pub is_image: Option<bool>,
    #[serde(rename = "backgroundTaskId")]
    pub background_task_id: Option<String>,
    #[serde(rename = "backgroundedByUser")]
    pub backgrounded_by_user: Option<bool>,
    #[serde(rename = "assistantAutoBackgrounded")]
    pub assistant_auto_backgrounded: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "returnCodeInterpretation")]
    pub return_code_interpretation: Option<String>,
    #[serde(rename = "noOutputExpected")]
    pub no_output_expected: Option<bool>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Option<Vec<serde_json::Value>>,
    #[serde(rename = "persistedOutputPath")]
    pub persisted_output_path: Option<String>,
    #[serde(rename = "persistedOutputSize")]
    pub persisted_output_size: Option<u64>,
    #[serde(rename = "sandboxStatus")]
    pub sandbox_status: Option<SandboxStatus>,
}

pub fn execute_bash(input: BashCommandInput) -> io::Result<BashCommandOutput> {
    let cmd = &input.command;
    let timeout = input.timeout;
    tracing::info!(command = %cmd, timeout_secs = timeout, "executing bash command");
    let cwd = env::current_dir()?;
    let sandbox_status = sandbox_status_for_input(&input, &cwd);

    if input.dangerously_disable_sandbox == Some(true) {
        tracing::warn!(
            "[WARNING] Sandbox disabled via dangerously_disable_sandbox for command: {}",
            input.command
        );
    }

    // Validate command before execution.
    let mode = match input.permission_mode.as_deref() {
        Some("read-only") => PermissionMode::ReadOnly,
        Some("workspace-write") => PermissionMode::WorkspaceWrite,
        Some("prompt") => PermissionMode::Prompt,
        Some("allow") => PermissionMode::Allow,
        None => PermissionMode::DangerFullAccess,
        Some(unknown) => {
            tracing::warn!("[WARNING] Unknown permission_mode '{unknown}', defaulting to read-only");
            PermissionMode::ReadOnly
        }
    };
    match bash_validation::validate_command(&input.command, mode, &cwd) {
        ValidationResult::Block { reason } => {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason));
        }
        ValidationResult::Warn { message } => {
            tracing::warn!("bash validation warning: {message}");
        }
        ValidationResult::Allow => {}
    }

    if input.run_in_background.unwrap_or(false) {
        let mut child = prepare_command(&input.command, &cwd, &sandbox_status, false);
        let mut child = child
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        let task_id = child.id().to_string();
        let task_id_for_removal = task_id.clone();
        let handle = std::thread::spawn(move || {
            let _ = child.wait();
            BACKGROUND_TASK_REGISTRY
                .lock()
                .expect("registry mutex poisoned")
                .remove(&task_id_for_removal);
        });

        BACKGROUND_TASK_REGISTRY
            .lock()
            .expect("registry mutex poisoned")
            .insert(task_id.clone(), handle);

        return Ok(BashCommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            raw_output_path: None,
            interrupted: false,
            is_image: None,
            background_task_id: Some(task_id),
            backgrounded_by_user: Some(false),
            assistant_auto_backgrounded: Some(false),
            dangerously_disable_sandbox: input.dangerously_disable_sandbox,
            return_code_interpretation: None,
            no_output_expected: Some(true),
            structured_content: None,
            persisted_output_path: None,
            persisted_output_size: None,
            sandbox_status: Some(sandbox_status),
        });
    }

    let runtime = Builder::new_current_thread().enable_all().build()?;
    runtime.block_on(execute_bash_async(input, sandbox_status, cwd))
}

#[allow(dead_code)]
/// Clean up finished background task handles from the registry.
///
/// Iterates over all tracked background tasks and joins those that have completed,
/// removing them from the registry. Running tasks are left untouched.
pub fn reap_background_tasks() {
    let mut registry = BACKGROUND_TASK_REGISTRY
        .lock()
        .expect("registry mutex poisoned");

    let finished: Vec<String> = registry
        .iter()
        .filter(|(_, handle)| handle.is_finished())
        .map(|(id, _)| id.clone())
        .collect();

    for id in finished {
        if let Some(handle) = registry.remove(&id) {
            let _ = handle.join();
        }
    }
}

async fn execute_bash_async(
    input: BashCommandInput,
    sandbox_status: SandboxStatus,
    cwd: std::path::PathBuf,
) -> io::Result<BashCommandOutput> {
    let mode = match input.permission_mode.as_deref() {
        Some("read-only") => PermissionMode::ReadOnly,
        Some("workspace-write") => PermissionMode::WorkspaceWrite,
        Some("prompt") => PermissionMode::Prompt,
        Some("allow") => PermissionMode::Allow,
        None => PermissionMode::DangerFullAccess,
        Some(unknown) => {
            tracing::warn!("[WARNING] Unknown permission_mode '{unknown}', defaulting to read-only");
            PermissionMode::ReadOnly
        }
    };
    match bash_validation::validate_command(&input.command, mode, &cwd) {
        ValidationResult::Block { reason } => {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason));
        }
        ValidationResult::Warn { message } => {
            tracing::warn!("bash validation warning: {message}");
        }
        ValidationResult::Allow => {}
    }

    let mut command = prepare_tokio_command(&input.command, &cwd, &sandbox_status, true);

    let output_result = if let Some(timeout_ms) = input.timeout {
        let child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let pid = child.id();
        if let Ok(result) =
            tokio::time::timeout(Duration::from_millis(timeout_ms), child.wait_with_output()).await
        {
            (result?, false)
        } else {
            if let Some(pid) = pid {
                let _ = std::process::Command::new("kill")
                    .args(["-9", &pid.to_string()])
                    .output();
            }
            tracing::warn!(timeout_ms = timeout_ms, "bash command timed out");
            return Ok(BashCommandOutput {
                stdout: String::new(),
                stderr: format!("Command exceeded timeout of {timeout_ms} ms. Partial output may have been discarded."),
                raw_output_path: None,
                interrupted: true,
                is_image: None,
                background_task_id: None,
                backgrounded_by_user: None,
                assistant_auto_backgrounded: None,
                dangerously_disable_sandbox: input.dangerously_disable_sandbox,
                return_code_interpretation: Some(String::from("timeout")),
                no_output_expected: Some(true),
                structured_content: None,
                persisted_output_path: None,
                persisted_output_size: None,
                sandbox_status: Some(sandbox_status),
            });
        }
    } else {
        (command.output().await?, false)
    };

    let (output, interrupted) = output_result;
    let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
    let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
    let code = output.status.code();
    tracing::info!(exit_code = ?code, stdout_bytes = stdout.len(), stderr_bytes = stderr.len(), "bash command completed");
    let no_output_expected = Some(stdout.trim().is_empty() && stderr.trim().is_empty());
    let return_code_interpretation = output.status.code().and_then(|code| {
        if code == 0 {
            None
        } else {
            Some(format!("exit_code:{code}"))
        }
    });

    Ok(BashCommandOutput {
        stdout,
        stderr,
        raw_output_path: None,
        interrupted,
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox: input.dangerously_disable_sandbox,
        return_code_interpretation,
        no_output_expected,
        structured_content: None,
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status: Some(sandbox_status),
    })
}

fn sandbox_status_for_input(input: &BashCommandInput, cwd: &std::path::Path) -> SandboxStatus {
    let config = ConfigLoader::default_for(cwd).load().map_or_else(
        |_| SandboxConfig::default(),
        |runtime_config| runtime_config.sandbox().clone(),
    );
    let request = config.resolve_request(
        input.dangerously_disable_sandbox.map(|disabled| !disabled),
        input.namespace_restrictions,
        input.isolate_network,
        input.filesystem_mode,
        input.allowed_mounts.clone(),
    );
    resolve_sandbox_status_for_request(&request, cwd)
}

fn prepare_command(
    command: &str,
    cwd: &std::path::Path,
    sandbox_status: &SandboxStatus,
    create_dirs: bool,
) -> Command {
    if create_dirs {
        prepare_sandbox_dirs(cwd);
    }

    if let Some(launcher) = build_linux_sandbox_command(command, cwd, sandbox_status) {
        let mut prepared = Command::new(launcher.program);
        prepared.args(launcher.args);
        prepared.current_dir(cwd);
        prepared.envs(launcher.env);
        return prepared;
    }

    let mut prepared = Command::new("sh");
    prepared.arg("-lc").arg(command).current_dir(cwd);
    if sandbox_status.filesystem_active {
        prepared.env("HOME", cwd.join(".sandbox-home"));
        prepared.env("TMPDIR", cwd.join(".sandbox-tmp"));
    }
    prepared
}

fn prepare_tokio_command(
    command: &str,
    cwd: &std::path::Path,
    sandbox_status: &SandboxStatus,
    create_dirs: bool,
) -> TokioCommand {
    if create_dirs {
        prepare_sandbox_dirs(cwd);
    }

    if let Some(launcher) = build_linux_sandbox_command(command, cwd, sandbox_status) {
        let mut prepared = TokioCommand::new(launcher.program);
        prepared.args(launcher.args);
        prepared.current_dir(cwd);
        prepared.envs(launcher.env);
        return prepared;
    }

    let mut prepared = TokioCommand::new("sh");
    prepared.arg("-lc").arg(command).current_dir(cwd);
    if sandbox_status.filesystem_active {
        prepared.env("HOME", cwd.join(".sandbox-home"));
        prepared.env("TMPDIR", cwd.join(".sandbox-tmp"));
    }
    prepared
}

fn prepare_sandbox_dirs(cwd: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(cwd.join(".sandbox-home")) {
        tracing::warn!("bash: failed to create sandbox HOME: {e}");
    }
    if let Err(e) = std::fs::create_dir_all(cwd.join(".sandbox-tmp")) {
        tracing::warn!("bash: failed to create sandbox TMPDIR: {e}");
    }
}

#[allow(dead_code)]
pub fn cleanup_sandbox_dirs(cwd: &std::path::Path) {
    let _ = std::fs::remove_dir_all(cwd.join(".sandbox-home"));
    let _ = std::fs::remove_dir_all(cwd.join(".sandbox-tmp"));
}

#[cfg(test)]
mod tests {
    use super::{execute_bash, BashCommandInput};
    use crate::sandbox::FilesystemIsolationMode;

    #[test]
    fn executes_simple_command() {
        let output = execute_bash(BashCommandInput {
            command: String::from("printf 'hello'"),
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(false),
            namespace_restrictions: Some(false),
            isolate_network: Some(false),
            filesystem_mode: Some(FilesystemIsolationMode::WorkspaceOnly),
            allowed_mounts: None,
            permission_mode: None,
        })
        .expect("bash command should execute");

        assert_eq!(output.stdout, "hello");
        assert!(!output.interrupted);
        assert!(output.sandbox_status.is_some());
    }

    #[test]
    fn disables_sandbox_when_requested() {
        let output = execute_bash(BashCommandInput {
            command: String::from("printf 'hello'"),
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(true),
            namespace_restrictions: None,
            isolate_network: None,
            filesystem_mode: None,
            allowed_mounts: None,
            permission_mode: None,
        })
        .expect("bash command should execute");

        assert!(!output.sandbox_status.expect("sandbox status").enabled);
    }
}

/// Maximum output bytes before truncation (16 KiB, matching upstream).
const MAX_OUTPUT_BYTES: usize = 16_384;

/// Truncate output to `MAX_OUTPUT_BYTES`, appending a marker when trimmed.
fn truncate_output(s: &str) -> String {
    if s.len() <= MAX_OUTPUT_BYTES {
        return s.to_string();
    }
    // Find the last valid UTF-8 boundary at or before MAX_OUTPUT_BYTES
    let mut end = MAX_OUTPUT_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = s[..end].to_string();
    truncated.push_str("\n\n[output truncated — exceeded 16384 bytes]");
    truncated
}

#[cfg(test)]
mod truncation_tests {
    use super::*;

    #[test]
    fn short_output_unchanged() {
        let s = "hello world";
        assert_eq!(truncate_output(s), s);
    }

    #[test]
    fn long_output_truncated() {
        let s = "x".repeat(20_000);
        let result = truncate_output(&s);
        assert!(result.len() < 20_000);
        assert!(result.ends_with("[output truncated — exceeded 16384 bytes]"));
    }

    #[test]
    fn exact_boundary_unchanged() {
        let s = "a".repeat(MAX_OUTPUT_BYTES);
        assert_eq!(truncate_output(&s), s);
    }

    #[test]
    fn one_over_boundary_truncated() {
        let s = "a".repeat(MAX_OUTPUT_BYTES + 1);
        let result = truncate_output(&s);
        assert!(result.contains("[output truncated"));
    }
}
