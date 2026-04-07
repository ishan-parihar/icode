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

fn assert_valid_json(stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8(stdout.to_vec()).expect("stdout should be utf8");
    serde_json::from_str(&text).expect("output should be valid JSON")
}

#[test]
fn version_json_output_is_valid() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["--output-format", "json", "--version"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    assert_eq!(value["kind"], "version");
    assert!(value["version"].is_string());
    assert!(value["message"].is_string());
}

#[test]
fn help_json_output_is_valid() {
    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .args(["--output-format", "json", "--help"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    assert_eq!(value["kind"], "help");
    assert!(value["message"].is_string());
}

#[test]
fn status_json_output_is_valid() {
    let temp_dir = unique_temp_dir("status-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "status"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    assert_eq!(value["kind"], "status");
    assert!(value["model"].is_string());
    assert!(value["permission_mode"].is_string());
    assert!(value["workspace"].is_object());
    assert!(value["sandbox"].is_object());

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn sandbox_json_output_is_valid() {
    let temp_dir = unique_temp_dir("sandbox-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "sandbox"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    assert_eq!(value["kind"], "sandbox");
    assert!(value["enabled"].is_boolean());
    assert!(value["active"].is_boolean());
    assert!(value["supported"].is_boolean());

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn doctor_json_output_is_valid() {
    let temp_dir = unique_temp_dir("doctor-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "doctor"])
        .output()
        .expect("icode should launch");

    // doctor may exit non-zero if there are failures, but JSON should still be valid
    let value = assert_valid_json(&output.stdout);
    assert_eq!(value["kind"], "doctor");
    assert!(value["has_failures"].is_boolean());
    assert!(value["checks"].is_array());

    let checks = value["checks"].as_array().expect("checks should be array");
    assert!(!checks.is_empty(), "should have at least one check");

    for check in checks {
        assert!(check["name"].is_string(), "each check should have a name");
        assert!(
            check["status"].is_string(),
            "each check should have a status"
        );
        assert!(
            check["summary"].is_string(),
            "each check should have a summary"
        );
        let status = check["status"].as_str().unwrap();
        assert!(
            matches!(status, "pass" | "warn" | "fail"),
            "status should be pass, warn, or fail: {status}"
        );
    }

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn agents_json_output_is_valid() {
    let temp_dir = unique_temp_dir("agents-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "agents"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    // agents returns an array of agent objects
    assert!(value.is_array(), "agents output should be a JSON array");

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn mcp_json_output_is_valid() {
    let temp_dir = unique_temp_dir("mcp-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "mcp"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    assert_eq!(value["kind"], "mcp");
    assert!(value["message"].is_string());

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
}

#[test]
fn skills_json_output_is_valid() {
    let temp_dir = unique_temp_dir("skills-json");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["--output-format", "json", "skills"])
        .output()
        .expect("icode should launch");

    assert_success(&output);
    let value = assert_valid_json(&output.stdout);
    // skills returns an array of skill objects
    assert!(value.is_array(), "skills output should be a JSON array");

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
fn text_output_still_works_for_doctor() {
    let temp_dir = unique_temp_dir("doctor-text");
    fs::create_dir_all(&temp_dir).expect("temp dir should exist");

    let output = Command::new(env!("CARGO_BIN_EXE_icode"))
        .current_dir(&temp_dir)
        .args(["doctor"])
        .output()
        .expect("icode should launch");

    // doctor may exit non-zero if there are failures
    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("Doctor"));
    assert!(stdout.contains("Summary"));

    fs::remove_dir_all(temp_dir).expect("cleanup temp dir");
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
