use std::fs;

use crate::constants::{BOULDER_DIR, NOTEPAD_DIR};
use crate::types::Notepad;

const NOTEPAD_FILES: &[&str] = &[
    "learnings.md",
    "decisions.md",
    "issues.md",
    "verification.md",
    "problems.md",
];

#[must_use]
pub fn notepad_dir(base: &str, plan_name: &str) -> String {
    format!("{base}/{BOULDER_DIR}/{NOTEPAD_DIR}/{plan_name}")
}

#[must_use]
pub fn read_notepad(base: &str, plan_name: &str) -> Option<Notepad> {
    let dir = notepad_dir(base, plan_name);

    let read_file =
        |name: &str| -> String { fs::read_to_string(format!("{dir}/{name}")).unwrap_or_default() };

    Some(Notepad {
        plan_name: plan_name.to_string(),
        learnings: read_file("learnings.md"),
        decisions: read_file("decisions.md"),
        issues: read_file("issues.md"),
        verification: read_file("verification.md"),
        problems: read_file("problems.md"),
    })
}

#[must_use]
pub fn write_notepad(base: &str, plan_name: &str, notepad: &Notepad) -> bool {
    let dir = notepad_dir(base, plan_name);
    if fs::create_dir_all(&dir).is_err() {
        return false;
    }

    let write = |name: &str, content: &str| fs::write(format!("{dir}/{name}"), content).is_ok();

    write("learnings.md", &notepad.learnings)
        && write("decisions.md", &notepad.decisions)
        && write("issues.md", &notepad.issues)
        && write("verification.md", &notepad.verification)
        && write("problems.md", &notepad.problems)
}

#[must_use]
pub fn create_notepad(base: &str, plan_name: &str) -> bool {
    let dir = notepad_dir(base, plan_name);
    if fs::create_dir_all(&dir).is_err() {
        return false;
    }

    NOTEPAD_FILES
        .iter()
        .all(|name| fs::write(format!("{dir}/{name}"), "").is_ok())
}
