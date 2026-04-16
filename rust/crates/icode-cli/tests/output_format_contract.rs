use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be valid")
        .as_millis();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("icode-test-{label}-{ts}-{counter}"))
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command should succeed. stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn version_command_remains_text_with_json_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["--output-format", "json", "--version"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("icode"));
    assert!(stdout.contains("Version"));
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "version output should remain text today"
    );
}

#[test]
fn help_command_remains_text_with_json_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["--output-format", "json", "--help"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--output-format FORMAT"));
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "help output should remain text today"
    );
}

#[test]
fn status_command_remains_text_with_json_flag() {
    let temp_dir = unique_temp_dir("status-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "status"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("Status"));
    assert!(stdout.contains("Model"));
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "status output should remain text today"
    );

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn sandbox_command_remains_text_with_json_flag() {
    let temp_dir = unique_temp_dir("sandbox-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "sandbox"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("Sandbox"));
    assert!(stdout.contains("Enabled"));
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "sandbox output should remain text today"
    );

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn text_output_still_works_for_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["--version"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("icode"));
    assert!(stdout.contains("Version"));
}

#[test]
fn text_output_still_works_for_status() {
    let temp_dir = unique_temp_dir("status-text");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["status"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("Status"));
    assert!(stdout.contains("Model"));

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn text_output_still_works_for_sandbox() {
    let temp_dir = unique_temp_dir("sandbox-text");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["sandbox"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("Sandbox"));
    assert!(stdout.contains("Enabled"));

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn doctor_top_level_command_returns_guidance_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["doctor"])
        .output()
        .expect("icode should launch");

    assert!(
        !output.status.success(),
        "top-level doctor should fail with guidance"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("`icode doctor` is a slash command"));
    assert!(stderr.contains("/doctor"));
}

#[test]
fn output_format_rejects_invalid_value() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["--output-format", "xml", "--version"])
        .output()
        .expect("icode should launch");

    assert!(
        !output.status.success(),
        "command should fail with invalid output format"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported value for --output-format"),
        "should mention unsupported format. stderr: {stderr}"
    );
}
