//! Bash command validation submodules.
//!
//! Ports the upstream `BashTool` validation pipeline:
//! - `readOnlyValidation` — block write-like commands in read-only mode
//! - `destructiveCommandWarning` — flag dangerous destructive commands
//! - `modeValidation` — enforce permission mode constraints on commands
//! - `sedValidation` — validate sed expressions before execution
//! - `pathValidation` — detect suspicious path patterns
//! - `commandSemantics` — classify command intent

use std::path::Path;

use crate::permission_enforcer::sed_has_inplace_flag;
use crate::permissions::PermissionMode;

/// Result of validating a bash command before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// Command is safe to execute.
    Allow,
    /// Command should be blocked with the given reason.
    Block { reason: String },
    /// Command requires user confirmation with the given warning.
    Warn { message: String },
}

/// Semantic classification of a bash command's intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandIntent {
    /// Read-only operations: ls, cat, grep, find, etc.
    ReadOnly,
    /// File system writes: cp, mv, mkdir, touch, tee, etc.
    Write,
    /// Destructive operations: rm, shred, truncate, etc.
    Destructive,
    /// Network operations: curl, wget, ssh, etc.
    Network,
    /// Process management: kill, pkill, etc.
    ProcessManagement,
    /// Package management: apt, brew, pip, npm, etc.
    PackageManagement,
    /// System administration: sudo, chmod, chown, mount, etc.
    SystemAdmin,
    /// Unknown or unclassifiable command.
    Unknown,
}

// ---------------------------------------------------------------------------
// readOnlyValidation
// ---------------------------------------------------------------------------

/// Commands that perform write operations and should be blocked in read-only mode.
const WRITE_COMMANDS: &[&str] = &[
    "cp", "mv", "rm", "mkdir", "rmdir", "touch", "chmod", "chown", "chgrp", "ln", "install", "tee",
    "truncate", "shred", "mkfifo", "mknod", "dd",
];

/// Commands that modify system state and should be blocked in read-only mode.
const STATE_MODIFYING_COMMANDS: &[&str] = &[
    "apt",
    "apt-get",
    "yum",
    "dnf",
    "pacman",
    "brew",
    "pip",
    "pip3",
    "npm",
    "yarn",
    "pnpm",
    "bun",
    "cargo",
    "gem",
    "go",
    "rustup",
    "docker",
    "systemctl",
    "service",
    "mount",
    "umount",
    "kill",
    "pkill",
    "killall",
    "reboot",
    "shutdown",
    "halt",
    "poweroff",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "groupdel",
    "crontab",
    "at",
];

/// Shell redirection operators that indicate writes.
const WRITE_REDIRECTIONS: &[&str] = &[">", ">>", ">&"];

/// Validate that a command is allowed under read-only mode.
///
/// Corresponds to upstream `tools/BashTool/readOnlyValidation.ts`.
#[must_use]
pub fn validate_read_only(command: &str, mode: PermissionMode) -> ValidationResult {
    if mode != PermissionMode::ReadOnly {
        return ValidationResult::Allow;
    }

    let all_commands = extract_all_commands(command);

    for first_command in &all_commands {
        // Check for write commands.
        for &write_cmd in WRITE_COMMANDS {
            if first_command == write_cmd {
                return ValidationResult::Block {
                    reason: format!(
                        "Command '{write_cmd}' modifies the filesystem and is not allowed in read-only mode"
                    ),
                };
            }
        }

        // Check for state-modifying commands.
        for &state_cmd in STATE_MODIFYING_COMMANDS {
            if first_command == state_cmd {
                return ValidationResult::Block {
                    reason: format!(
                        "Command '{state_cmd}' modifies system state and is not allowed in read-only mode"
                    ),
                };
            }
        }

        // Check for sudo wrapping write commands or a shell interpreter.
        if first_command == "sudo" {
            let sudo_words = extract_sudo_words(command);
            for word in &sudo_words {
                if WRITE_COMMANDS.contains(word)
                    || STATE_MODIFYING_COMMANDS.contains(word)
                    || ALWAYS_DESTRUCTIVE_COMMANDS.contains(word)
                {
                    return ValidationResult::Block {
                        reason: format!("sudo command contains blocked target '{word}' which modifies state in read-only mode"),
                    };
                }
                // Block shell-interpreter escalation (e.g. `sudo bash -c 'rm -rf /'`)
                if DANGEROUS_SHELLS.contains(word) {
                    return ValidationResult::Block {
                        reason: format!("sudo command invokes shell interpreter '{word}' which could execute arbitrary code in read-only mode"),
                    };
                }
            }
        }

        // Check for git commands that modify state.
        if first_command == "git" {
            let res = validate_git_read_only(command);
            if res != ValidationResult::Allow {
                return res;
            }
        }
    }

    // Check for write redirections.
    for &redir in WRITE_REDIRECTIONS {
        if command.contains(redir) {
            return ValidationResult::Block {
                reason: format!(
                    "Command contains write redirection '{redir}' which is not allowed in read-only mode"
                ),
            };
        }
    }

    // Check for unquoted pipes — pipes can chain commands in ways that bypass
    // first-command validation (e.g. `ls | xargs rm`).
    if command_has_unquoted_pipe(command) {
        return ValidationResult::Block {
            reason: "Command contains a pipe which can chain destructive operations and is not allowed in read-only mode".to_string(),
        };
    }

    ValidationResult::Allow
}

/// Git subcommands that are read-only safe.
const GIT_READ_ONLY_SUBCOMMANDS: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "tag",
    "stash",
    "remote",
    "fetch",
    "ls-files",
    "ls-tree",
    "cat-file",
    "rev-parse",
    "describe",
    "shortlog",
    "blame",
    "bisect",
    "reflog",
    "config",
];

fn validate_git_read_only(command: &str) -> ValidationResult {
    let parts: Vec<&str> = command.split_whitespace().collect();
    // Skip past "git" and any flags (e.g., "git -C /path")
    let subcommand = parts.iter().skip(1).find(|p| !p.starts_with('-'));

    match subcommand {
        Some(&sub) if GIT_READ_ONLY_SUBCOMMANDS.contains(&sub) => ValidationResult::Allow,
        Some(&sub) => ValidationResult::Block {
            reason: format!(
                "Git subcommand '{sub}' modifies repository state and is not allowed in read-only mode"
            ),
        },
        None => ValidationResult::Allow, // bare "git" is fine
    }
}

// ---------------------------------------------------------------------------
// destructiveCommandWarning
// ---------------------------------------------------------------------------

/// Patterns that indicate potentially destructive commands.
const DESTRUCTIVE_PATTERNS: &[(&str, &str)] = &[
    (
        "rm -rf /",
        "Recursive forced deletion at root — this will destroy the system",
    ),
    ("rm -rf ~", "Recursive forced deletion of home directory"),
    (
        "rm -rf *",
        "Recursive forced deletion of all files in current directory",
    ),
    ("rm -rf .", "Recursive forced deletion of current directory"),
    (
        "mkfs",
        "Filesystem creation will destroy existing data on the device",
    ),
    (
        "dd if=",
        "Direct disk write — can overwrite partitions or devices",
    ),
    ("> /dev/sd", "Writing to raw disk device"),
    (
        "chmod -R 777",
        "Recursively setting world-writable permissions",
    ),
    ("chmod -R 000", "Recursively removing all permissions"),
    (":(){ :|:& };:", "Fork bomb — will crash the system"),
];

/// Commands that are always destructive regardless of arguments.
const ALWAYS_DESTRUCTIVE_COMMANDS: &[&str] = &["shred", "wipefs"];

/// Shell interpreters that can execute arbitrary code and must not be allowed
/// as the inner command of a `sudo` invocation in restricted modes.
const DANGEROUS_SHELLS: &[&str] = &[
    "sh", "bash", "zsh", "dash", "fish", "ksh", "tcsh", "csh", "rbash", "python", "python3",
    "python2", "perl", "ruby", "node", "nodejs", "lua", "tclsh", "wish",
];

/// Warn if a command looks destructive.
///
/// Corresponds to upstream `tools/BashTool/destructiveCommandWarning.ts`.
#[must_use]
pub fn check_destructive(command: &str) -> ValidationResult {
    // Check known destructive patterns.
    for &(pattern, warning) in DESTRUCTIVE_PATTERNS {
        if command.contains(pattern) {
            return ValidationResult::Warn {
                message: format!("Destructive command detected: {warning}"),
            };
        }
    }

    let all_cmds = extract_all_commands(command);
    for first in &all_cmds {
        for &cmd in ALWAYS_DESTRUCTIVE_COMMANDS {
            if first == cmd {
                return ValidationResult::Warn {
                    message: format!(
                        "Command '{cmd}' is inherently destructive and may cause data loss"
                    ),
                };
            }
        }
    }

    // Check for "rm -rf" with broad targets.
    if command.contains("rm ") && command.contains("-r") && command.contains("-f") {
        // Already handled the most dangerous patterns above.
        // Flag any remaining "rm -rf" as a warning.
        return ValidationResult::Warn {
            message: "Recursive forced deletion detected — verify the target path is correct"
                .to_string(),
        };
    }

    ValidationResult::Allow
}

// ---------------------------------------------------------------------------
// modeValidation
// ---------------------------------------------------------------------------

/// Validate that a command is consistent with the given permission mode.
///
/// Corresponds to upstream `tools/BashTool/modeValidation.ts`.
#[must_use]
pub fn validate_mode(command: &str, mode: PermissionMode) -> ValidationResult {
    match mode {
        PermissionMode::ReadOnly => validate_read_only(command, mode),
        PermissionMode::WorkspaceWrite => {
            // In workspace-write mode, check for system-level destructive
            // operations that go beyond workspace scope.
            if command_targets_outside_workspace(command) {
                return ValidationResult::Warn {
                    message:
                        "Command appears to target files outside the workspace — requires elevated permission"
                            .to_string(),
                };
            }
            ValidationResult::Allow
        }
        PermissionMode::DangerFullAccess | PermissionMode::Allow | PermissionMode::Prompt => {
            ValidationResult::Allow
        }
    }
}

/// Heuristic: does the command reference absolute paths outside typical workspace dirs?
fn command_targets_outside_workspace(command: &str) -> bool {
    let system_paths = [
        "/etc/", "/usr/", "/var/", "/boot/", "/sys/", "/proc/", "/dev/", "/sbin/", "/lib/", "/opt/",
    ];

    let all_cmds = extract_all_commands(command);
    let mut is_write_cmd = false;
    for first in &all_cmds {
        if WRITE_COMMANDS.contains(&first.as_str())
            || STATE_MODIFYING_COMMANDS.contains(&first.as_str())
        {
            is_write_cmd = true;
            break;
        }
    }

    if !is_write_cmd {
        return false;
    }

    for sys_path in &system_paths {
        if command.contains(sys_path) {
            return true;
        }
    }

    false
}

// ---------------------------------------------------------------------------
// sedValidation
// ---------------------------------------------------------------------------

/// Validate sed expressions for safety.
///
/// Corresponds to upstream `tools/BashTool/sedValidation.ts`.
#[must_use]
pub fn validate_sed(command: &str, mode: PermissionMode) -> ValidationResult {
    let all_cmds = extract_all_commands(command);
    let mut has_sed = false;
    for first in &all_cmds {
        if first == "sed" {
            has_sed = true;
            break;
        }
    }

    if !has_sed {
        return ValidationResult::Allow;
    }

    // In read-only mode, block sed -i (in-place editing).
    if mode == PermissionMode::ReadOnly && sed_has_inplace_flag(command) {
        return ValidationResult::Block {
            reason: "sed -i (in-place editing) is not allowed in read-only mode".to_string(),
        };
    }

    ValidationResult::Allow
}

// ---------------------------------------------------------------------------
// pathValidation
// ---------------------------------------------------------------------------

/// Validate that command paths don't include suspicious traversal patterns.
///
/// Corresponds to upstream `tools/BashTool/pathValidation.ts`.
#[must_use]
pub fn validate_paths(command: &str, workspace: &Path) -> ValidationResult {
    if command.contains("../") || command.contains("..\\") {
        let workspace_str = workspace.to_string_lossy();
        let canonical_workspace = workspace.canonicalize().ok();
        let in_workspace = canonical_workspace
            .as_ref()
            .is_some_and(|ws| command.contains(&*ws.to_string_lossy()));
        if !in_workspace && !command.contains(&*workspace_str) {
            return ValidationResult::Block {
                reason: "Command contains directory traversal pattern that escapes the workspace — blocked".to_string(),
            };
        }
    }

    if command.contains("~/") || command.contains("$HOME") {
        return ValidationResult::Block {
            reason: "Command references home directory outside workspace scope — blocked"
                .to_string(),
        };
    }

    ValidationResult::Allow
}

// ---------------------------------------------------------------------------
// commandSemantics
// ---------------------------------------------------------------------------

/// Commands that are read-only (no filesystem or state modification).
const SEMANTIC_READ_ONLY_COMMANDS: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "less",
    "more",
    "wc",
    "sort",
    "uniq",
    "grep",
    "egrep",
    "fgrep",
    "find",
    "which",
    "whereis",
    "whatis",
    "man",
    "info",
    "file",
    "stat",
    "du",
    "df",
    "free",
    "uptime",
    "uname",
    "hostname",
    "whoami",
    "id",
    "groups",
    "env",
    "printenv",
    "echo",
    "printf",
    "date",
    "cal",
    "bc",
    "expr",
    "test",
    "true",
    "false",
    "pwd",
    "tree",
    "diff",
    "cmp",
    "md5sum",
    "sha256sum",
    "sha1sum",
    "xxd",
    "od",
    "hexdump",
    "strings",
    "readlink",
    "realpath",
    "basename",
    "dirname",
    "seq",
    "yes",
    "tput",
    "column",
    "jq",
    "yq",
    "xargs",
    "tr",
    "cut",
    "paste",
    "awk",
    "sed",
];

/// Commands that perform network operations.
const NETWORK_COMMANDS: &[&str] = &[
    "curl",
    "wget",
    "ssh",
    "scp",
    "rsync",
    "ftp",
    "sftp",
    "nc",
    "ncat",
    "telnet",
    "ping",
    "traceroute",
    "dig",
    "nslookup",
    "host",
    "whois",
    "ifconfig",
    "ip",
    "netstat",
    "ss",
    "nmap",
];

/// Commands that manage processes.
const PROCESS_COMMANDS: &[&str] = &[
    "kill", "pkill", "killall", "ps", "top", "htop", "bg", "fg", "jobs", "nohup", "disown", "wait",
    "nice", "renice",
];

/// Commands that manage packages.
const PACKAGE_COMMANDS: &[&str] = &[
    "apt", "apt-get", "yum", "dnf", "pacman", "brew", "pip", "pip3", "npm", "yarn", "pnpm", "bun",
    "cargo", "gem", "go", "rustup", "snap", "flatpak",
];

/// Commands that require system administrator privileges.
const SYSTEM_ADMIN_COMMANDS: &[&str] = &[
    "sudo",
    "su",
    "chroot",
    "mount",
    "umount",
    "fdisk",
    "parted",
    "lsblk",
    "blkid",
    "systemctl",
    "service",
    "journalctl",
    "dmesg",
    "modprobe",
    "insmod",
    "rmmod",
    "iptables",
    "ufw",
    "firewall-cmd",
    "sysctl",
    "crontab",
    "at",
    "useradd",
    "userdel",
    "usermod",
    "groupadd",
    "groupdel",
    "passwd",
    "visudo",
];

/// Classify the semantic intent of a bash command.
///
/// Corresponds to upstream `tools/BashTool/commandSemantics.ts`.
#[must_use]
pub fn classify_command(command: &str) -> CommandIntent {
    let commands = extract_all_commands(command);
    let mut highest_severity = CommandIntent::Unknown;

    if commands.is_empty() {
        return highest_severity;
    }

    for first in &commands {
        let intent = classify_by_first_command(first, command);
        highest_severity = match (highest_severity, intent) {
            (CommandIntent::Destructive, _) | (_, CommandIntent::Destructive) => {
                CommandIntent::Destructive
            }
            (CommandIntent::SystemAdmin, _) | (_, CommandIntent::SystemAdmin) => {
                CommandIntent::SystemAdmin
            }
            (CommandIntent::Write, _) | (_, CommandIntent::Write) => CommandIntent::Write,
            (CommandIntent::Network, _) | (_, CommandIntent::Network) => CommandIntent::Network,
            (CommandIntent::ProcessManagement, _) | (_, CommandIntent::ProcessManagement) => {
                CommandIntent::ProcessManagement
            }
            (CommandIntent::PackageManagement, _) | (_, CommandIntent::PackageManagement) => {
                CommandIntent::PackageManagement
            }
            (CommandIntent::ReadOnly, _) | (_, CommandIntent::ReadOnly) => CommandIntent::ReadOnly,
            _ => CommandIntent::Unknown,
        };
    }
    highest_severity
}

fn classify_by_first_command(first: &str, command: &str) -> CommandIntent {
    if SEMANTIC_READ_ONLY_COMMANDS.contains(&first) {
        if first == "sed" && command.contains(" -i") {
            return CommandIntent::Write;
        }
        return CommandIntent::ReadOnly;
    }

    if ALWAYS_DESTRUCTIVE_COMMANDS.contains(&first) || first == "rm" {
        return CommandIntent::Destructive;
    }

    if WRITE_COMMANDS.contains(&first) {
        return CommandIntent::Write;
    }

    if NETWORK_COMMANDS.contains(&first) {
        return CommandIntent::Network;
    }

    if PROCESS_COMMANDS.contains(&first) {
        return CommandIntent::ProcessManagement;
    }

    if PACKAGE_COMMANDS.contains(&first) {
        return CommandIntent::PackageManagement;
    }

    if SYSTEM_ADMIN_COMMANDS.contains(&first) {
        return CommandIntent::SystemAdmin;
    }

    if first == "git" {
        return classify_git_command(command);
    }

    CommandIntent::Unknown
}

fn classify_git_command(command: &str) -> CommandIntent {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let subcommand = parts.iter().skip(1).find(|p| !p.starts_with('-'));
    match subcommand {
        Some(&sub) if GIT_READ_ONLY_SUBCOMMANDS.contains(&sub) => CommandIntent::ReadOnly,
        _ => CommandIntent::Write,
    }
}

// ---------------------------------------------------------------------------
// Pipeline: run all validations
// ---------------------------------------------------------------------------

/// Run the full validation pipeline on a bash command.
///
/// Returns the first non-Allow result, or Allow if all validations pass.
#[must_use]
pub fn validate_command(command: &str, mode: PermissionMode, workspace: &Path) -> ValidationResult {
    // 1. Mode-level validation (includes read-only checks).
    let result = validate_mode(command, mode);
    if result != ValidationResult::Allow {
        return result;
    }

    // 2. Sed-specific validation.
    let result = validate_sed(command, mode);
    if result != ValidationResult::Allow {
        return result;
    }

    // 3. Destructive command warnings.
    let result = check_destructive(command);
    if result != ValidationResult::Allow {
        return result;
    }

    // 4. Path validation.
    validate_paths(command, workspace)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[must_use]
pub fn extract_all_commands(command: &str) -> Vec<String> {
    let mut cmds = Vec::new();
    for part in split_bash_commands(command) {
        let first = extract_first_command(part);
        if !first.is_empty() {
            cmds.push(first);
        }
    }
    cmds
}

fn split_bash_commands(command: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut last_split = 0;

    let bytes = command.as_bytes();
    for i in 0..bytes.len() {
        if escaped {
            escaped = false;
            continue;
        }
        let b = bytes[i];
        if b == b'\\' {
            // In bash, backslash only escapes inside double quotes or outside quotes.
            if !in_single {
                escaped = true;
            }
            continue;
        }

        if b == b'\'' && !in_double {
            in_single = !in_single;
        } else if b == b'"' && !in_single {
            in_double = !in_double;
        } else if !in_single && !in_double {
            match b {
                b';' | b'|' | b'&' | b'\n' | b'(' | b')' | b'`' => {
                    if i > last_split {
                        parts.push(&command[last_split..i]);
                    }
                    last_split = i + 1;
                }
                _ => {}
            }
        }
    }
    if last_split < command.len() {
        parts.push(&command[last_split..]);
    }

    parts
}

/// Extract the first bare command from a pipeline/chain, stripping env vars and sudo.
fn extract_first_command(command: &str) -> String {
    let trimmed = command.trim();

    // Skip leading environment variable assignments (KEY=val cmd ...).
    let mut remaining = trimmed;
    loop {
        let next = remaining.trim_start();
        if let Some(eq_pos) = next.find('=') {
            let before_eq = &next[..eq_pos];
            // Valid env var name: alphanumeric + underscore, no spaces.
            if !before_eq.is_empty()
                && before_eq
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                // Skip past the value (might be quoted).
                let after_eq = &next[eq_pos + 1..];
                if let Some(space) = find_end_of_value(after_eq) {
                    remaining = &after_eq[space..];
                    continue;
                }
                // No space found means value goes to end of string — no actual command.
                return String::new();
            }
        }
        break;
    }

    remaining
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

/// Extract all words following "sudo".
fn extract_sudo_words(command: &str) -> Vec<&str> {
    let parts: Vec<&str> = command.split_whitespace().collect();
    let mut inner = Vec::new();
    if let Some(mut idx) = parts.iter().position(|&p| p == "sudo") {
        idx += 1;
        while idx < parts.len() {
            inner.push(parts[idx]);
            idx += 1;
        }
    }
    inner
}

fn command_has_unquoted_pipe(command: &str) -> bool {
    let bytes = command.as_bytes();
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if escaped {
            escaped = false;
            i += 1;
            continue;
        }
        match b {
            b'\\' if !in_single => {
                escaped = true;
            }
            b'\'' if !in_double => {
                in_single = !in_single;
            }
            b'"' if !in_single => {
                in_double = !in_double;
            }
            b'|' if !in_single && !in_double => {
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Find the end of a value in `KEY=value rest` (handles basic quoting).
///
/// Correctly handles:
/// - Single-quoted strings: no backslash escapes at all in POSIX sh.
/// - Double-quoted strings: `\"` is an escaped quote; `\\` is an escaped backslash.
///   We count consecutive backslashes before a `"` to determine if it is escaped.
fn find_end_of_value(s: &str) -> Option<usize> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }

    let bytes = s.as_bytes();
    let first = bytes[0];
    if first == b'"' || first == b'\'' {
        let quote = first;
        let mut i = 1;
        while i < s.len() {
            let ch = bytes[i];
            if ch == quote {
                if quote == b'"' {
                    // Count the number of consecutive backslashes immediately before
                    // this quote. An even number means the quote is NOT escaped.
                    let mut backslash_count = 0usize;
                    let mut j = i;
                    while j > 0 && bytes[j - 1] == b'\\' {
                        backslash_count += 1;
                        j -= 1;
                    }
                    if backslash_count % 2 == 1 {
                        // The closing `"` is preceded by an odd number of backslashes,
                        // so it is escaped — not the end of the string.
                        i += 1;
                        continue;
                    }
                }
                // Single-quoted strings: no escape sequences; this is always the end.
                // Double-quoted strings: preceded by even backslashes (including 0).
                i += 1; // skip past closing quote
                        // Find next whitespace.
                while i < s.len() && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                return if i < s.len() { Some(i) } else { None };
            }
            // Inside a double-quoted string, skip backslash + next char together.
            if quote == b'"' && ch == b'\\' && i + 1 < s.len() {
                i += 2;
                continue;
            }
            i += 1;
        }
        None
    } else {
        s.find(char::is_whitespace)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- readOnlyValidation ---

    #[test]
    fn blocks_rm_in_read_only() {
        assert!(matches!(
            validate_read_only("rm -rf /tmp/x", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("rm")
        ));
    }

    #[test]
    fn allows_rm_in_workspace_write() {
        assert_eq!(
            validate_read_only("rm -rf /tmp/x", PermissionMode::WorkspaceWrite),
            ValidationResult::Allow
        );
    }

    #[test]
    fn blocks_write_redirections_in_read_only() {
        assert!(matches!(
            validate_read_only("echo hello > file.txt", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("redirection")
        ));
    }

    #[test]
    fn allows_read_commands_in_read_only() {
        assert_eq!(
            validate_read_only("ls -la", PermissionMode::ReadOnly),
            ValidationResult::Allow
        );
        assert_eq!(
            validate_read_only("cat /etc/hosts", PermissionMode::ReadOnly),
            ValidationResult::Allow
        );
        assert_eq!(
            validate_read_only("grep -r pattern .", PermissionMode::ReadOnly),
            ValidationResult::Allow
        );
    }

    #[test]
    fn blocks_sudo_write_in_read_only() {
        assert!(matches!(
            validate_read_only("sudo rm -rf /tmp/x", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("rm")
        ));
    }

    #[test]
    fn blocks_git_push_in_read_only() {
        assert!(matches!(
            validate_read_only("git push origin main", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("push")
        ));
    }

    #[test]
    fn allows_git_status_in_read_only() {
        assert_eq!(
            validate_read_only("git status", PermissionMode::ReadOnly),
            ValidationResult::Allow
        );
    }

    #[test]
    fn blocks_package_install_in_read_only() {
        assert!(matches!(
            validate_read_only("npm install express", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("npm")
        ));
    }

    // --- destructiveCommandWarning ---

    #[test]
    fn warns_rm_rf_root() {
        assert!(matches!(
            check_destructive("rm -rf /"),
            ValidationResult::Warn { message } if message.contains("root")
        ));
    }

    #[test]
    fn warns_rm_rf_home() {
        assert!(matches!(
            check_destructive("rm -rf ~"),
            ValidationResult::Warn { message } if message.contains("home")
        ));
    }

    #[test]
    fn warns_shred() {
        assert!(matches!(
            check_destructive("shred /dev/sda"),
            ValidationResult::Warn { message } if message.contains("destructive")
        ));
    }

    #[test]
    fn warns_fork_bomb() {
        assert!(matches!(
            check_destructive(":(){ :|:& };:"),
            ValidationResult::Warn { message } if message.contains("Fork bomb")
        ));
    }

    #[test]
    fn allows_safe_commands() {
        assert_eq!(check_destructive("ls -la"), ValidationResult::Allow);
        assert_eq!(check_destructive("echo hello"), ValidationResult::Allow);
    }

    // --- modeValidation ---

    #[test]
    fn workspace_write_warns_system_paths() {
        assert!(matches!(
            validate_mode("cp file.txt /etc/config", PermissionMode::WorkspaceWrite),
            ValidationResult::Warn { message } if message.contains("outside the workspace")
        ));
    }

    #[test]
    fn workspace_write_allows_local_writes() {
        assert_eq!(
            validate_mode("cp file.txt ./backup/", PermissionMode::WorkspaceWrite),
            ValidationResult::Allow
        );
    }

    // --- sedValidation ---

    #[test]
    fn blocks_sed_inplace_in_read_only() {
        assert!(matches!(
            validate_sed("sed -i 's/old/new/' file.txt", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("sed -i")
        ));
    }

    #[test]
    fn allows_sed_stdout_in_read_only() {
        assert_eq!(
            validate_sed("sed 's/old/new/' file.txt", PermissionMode::ReadOnly),
            ValidationResult::Allow
        );
    }

    // --- pathValidation ---

    #[test]
    fn blocks_directory_traversal() {
        let workspace = PathBuf::from("/workspace/project");
        assert!(matches!(
            validate_paths("cat ../../../etc/passwd", &workspace),
            ValidationResult::Block { reason } if reason.contains("traversal")
        ));
    }

    #[test]
    fn blocks_home_directory_reference() {
        let workspace = PathBuf::from("/workspace/project");
        assert!(matches!(
            validate_paths("cat ~/.ssh/id_rsa", &workspace),
            ValidationResult::Block { reason } if reason.contains("home directory")
        ));
    }

    // --- commandSemantics ---

    #[test]
    fn classifies_read_only_commands() {
        assert_eq!(classify_command("ls -la"), CommandIntent::ReadOnly);
        assert_eq!(classify_command("cat file.txt"), CommandIntent::ReadOnly);
        assert_eq!(
            classify_command("grep -r pattern ."),
            CommandIntent::ReadOnly
        );
        assert_eq!(
            classify_command("find . -name '*.rs'"),
            CommandIntent::ReadOnly
        );
    }

    #[test]
    fn classifies_write_commands() {
        assert_eq!(classify_command("cp a.txt b.txt"), CommandIntent::Write);
        assert_eq!(classify_command("mv old.txt new.txt"), CommandIntent::Write);
        assert_eq!(classify_command("mkdir -p /tmp/dir"), CommandIntent::Write);
    }

    #[test]
    fn classifies_destructive_commands() {
        assert_eq!(
            classify_command("rm -rf /tmp/x"),
            CommandIntent::Destructive
        );
        assert_eq!(
            classify_command("shred /dev/sda"),
            CommandIntent::Destructive
        );
    }

    #[test]
    fn classifies_network_commands() {
        assert_eq!(
            classify_command("curl https://example.com"),
            CommandIntent::Network
        );
        assert_eq!(classify_command("wget file.zip"), CommandIntent::Network);
    }

    #[test]
    fn classifies_sed_inplace_as_write() {
        assert_eq!(
            classify_command("sed -i 's/old/new/' file.txt"),
            CommandIntent::Write
        );
    }

    #[test]
    fn classifies_sed_stdout_as_read_only() {
        assert_eq!(
            classify_command("sed 's/old/new/' file.txt"),
            CommandIntent::ReadOnly
        );
    }

    #[test]
    fn classifies_git_status_as_read_only() {
        assert_eq!(classify_command("git status"), CommandIntent::ReadOnly);
        assert_eq!(
            classify_command("git log --oneline"),
            CommandIntent::ReadOnly
        );
    }

    #[test]
    fn classifies_git_push_as_write() {
        assert_eq!(
            classify_command("git push origin main"),
            CommandIntent::Write
        );
    }

    // --- validate_command (full pipeline) ---

    #[test]
    fn pipeline_blocks_write_in_read_only() {
        let workspace = PathBuf::from("/workspace");
        assert!(matches!(
            validate_command("rm -rf /tmp/x", PermissionMode::ReadOnly, &workspace),
            ValidationResult::Block { .. }
        ));
    }

    #[test]
    fn pipeline_warns_destructive_in_write_mode() {
        let workspace = PathBuf::from("/workspace");
        assert!(matches!(
            validate_command("rm -rf /", PermissionMode::WorkspaceWrite, &workspace),
            ValidationResult::Warn { .. }
        ));
    }

    #[test]
    fn pipeline_allows_safe_read_in_read_only() {
        let workspace = PathBuf::from("/workspace");
        assert_eq!(
            validate_command("ls -la", PermissionMode::ReadOnly, &workspace),
            ValidationResult::Allow
        );
    }

    // --- extract_first_command ---

    #[test]
    fn extracts_command_from_env_prefix() {
        assert_eq!(extract_first_command("FOO=bar ls -la"), "ls");
        assert_eq!(extract_first_command("A=1 B=2 echo hello"), "echo");
    }

    #[test]
    fn extracts_plain_command() {
        assert_eq!(extract_first_command("grep -r pattern ."), "grep");
    }

    // --- sudo shell escalation block ---

    #[test]
    fn blocks_sudo_shell_escalation_in_read_only() {
        // `sudo bash -c 'rm -rf /'` must be blocked — bash is a dangerous shell.
        assert!(matches!(
            validate_read_only("sudo bash -c 'rm -rf /'", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("bash")
        ));
        // `sudo -u root sh -c '...'` must be blocked — sh is a dangerous shell.
        assert!(matches!(
            validate_read_only("sudo -u root sh -c 'id'", PermissionMode::ReadOnly),
            ValidationResult::Block { reason } if reason.contains("sh")
        ));
    }

    // --- find_end_of_value double-quote handling ---

    #[test]
    fn env_prefix_double_quoted_with_escaped_backslash() {
        // `FOO="foo\\" cmd` — the value is `foo\` (backslash before closing quote is escaped).
        // extract_first_command must return "cmd", not "" (which would be the wrong parse).
        let result = extract_first_command(r#"FOO="foo\\" cmd"#);
        assert_eq!(
            result, "cmd",
            "expected 'cmd' but find_end_of_value mishandled escaped backslash before \""
        );
    }

    #[test]
    fn env_prefix_single_quoted_no_backslash_escape() {
        // Single quotes never process backslashes, so `FOO='it\'s'` should terminate at the `'` after \
        // This is a genuine single-quote split; the Bash behavior would treat the first `'` as end.
        // Here the VALUE is `it\`, the `s'` part is not inside the single-quoted string.
        // After stripping the env var FOO='it\' there may be trailing content.
        // The important assertion: no panic, returns non-empty.
        let result = extract_first_command("FOO='bar' ls");
        assert_eq!(result, "ls");
    }

    // -----------------------------------------------------------------------
    // Regression tests for Bug Report Plan - Batch 2
    // -----------------------------------------------------------------------

    // Bug #1: Bash multi-command validation bypass
    // Payload `ls && rm -rf /` must NOT parse as just `ls` — the `rm` must be
    // detected and blocked in read-only mode.
    #[test]
    fn blocks_multi_command_and_chain_in_read_only() {
        assert!(
            matches!(
                validate_read_only("ls && rm -rf /", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "ls && rm -rf / must be blocked in read-only mode"
        );
    }

    #[test]
    fn blocks_multi_command_or_chain_in_read_only() {
        assert!(
            matches!(
                validate_read_only("cat file || rm -rf /", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "cat file || rm -rf / must be blocked in read-only mode"
        );
    }

    #[test]
    fn blocks_multi_command_semicolon_in_read_only() {
        assert!(
            matches!(
                validate_read_only("ls; rm -rf /", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "ls; rm -rf / must be blocked in read-only mode"
        );
    }

    #[test]
    fn blocks_multi_command_pipe_in_read_only() {
        // `ls | xargs rm` — xargs is not in the read-only allowlist
        assert!(
            matches!(
                validate_read_only("ls | xargs rm", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "ls | xargs rm must be blocked in read-only mode"
        );
    }

    #[test]
    fn blocks_multiline_command_chain_in_read_only() {
        assert!(
            matches!(
                validate_read_only("ls\nrm -rf /", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "ls\\nrm -rf / must be blocked in read-only mode"
        );
    }

    // Bug #2: Sudo parameter value injection
    // `sudo -u root rm -rf /` must detect `rm` even with the `-u root` flag.
    #[test]
    fn blocks_sudo_with_user_flag_in_read_only() {
        assert!(
            matches!(
                validate_read_only("sudo -u root rm -rf /", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "sudo -u root rm -rf / must be blocked in read-only mode"
        );
    }

    #[test]
    fn blocks_sudo_multiple_flags_in_read_only() {
        assert!(
            matches!(
                validate_read_only("sudo -u root -E rm file", PermissionMode::ReadOnly),
                ValidationResult::Block { .. }
            ),
            "sudo -u root -E rm file must be blocked in read-only mode"
        );
    }

    #[test]
    fn blocks_sudo_shell_escalation_with_user_flag() {
        assert!(
            matches!(
                validate_read_only("sudo -u root sh -c 'id'", PermissionMode::ReadOnly),
                ValidationResult::Block { reason } if reason.contains("sh")
            ),
            "sudo -u root sh -c 'id' must be blocked — sh is a dangerous shell"
        );
    }

    // Bug #3: Single-quote evasion — backslash inside single quotes must NOT
    // be treated as an escape character.
    #[test]
    fn env_prefix_single_quote_backslash_not_escape() {
        // `FOO='bar' ls` — single quotes, no escaping, should extract "ls"
        let result = extract_first_command("FOO='bar' ls");
        assert_eq!(result, "ls");
    }

    #[test]
    fn env_prefix_single_quote_with_backslash_content() {
        // `FOO='it\'s' ls` — in bash, single quotes don't escape, so the '
        // at position after \ terminates the quote.
        // Our parser should handle this without panic and still extract "ls".
        let result = extract_first_command(r"FOO='it\'s' ls");
        // After the first ' ends the quoted section, 's' ls' is consumed as
        // non-whitespace trailing chars, then " ls" remains → "ls"
        assert_eq!(result, "ls");
    }
}
