use std::fs;
use std::path::Path;

use crate::constants::PROMETHEUS_PLANS_DIR;
use crate::types::{PlanProgress, TaskSection, TopLevelTask};

#[must_use]
pub fn get_plan_progress(plan_path: &str) -> PlanProgress {
    let Ok(content) = fs::read_to_string(plan_path) else {
        return PlanProgress {
            total: 0,
            completed: 0,
            is_complete: false,
        };
    };

    let mut total = 0usize;
    let mut completed = 0usize;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- [x]") || trimmed.starts_with("* [x]") {
            total += 1;
            completed += 1;
        } else if trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]") {
            total += 1;
        }
    }

    PlanProgress {
        total,
        completed,
        is_complete: total > 0 && total == completed,
    }
}

#[must_use]
pub fn get_plan_name(plan_path: &str) -> String {
    Path::new(plan_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[must_use]
pub fn find_prometheus_plans(directory: &str) -> Vec<String> {
    let plans_dir = format!("{directory}/{PROMETHEUS_PLANS_DIR}");

    let Ok(reader) = fs::read_dir(&plans_dir) else {
        return Vec::new();
    };

    let mut entries: Vec<_> = reader
        .filter_map(std::result::Result::ok)
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .collect();

    entries.sort_by(|a, b| {
        let a_mtime = a.metadata().ok().and_then(|m| m.modified().ok());
        let b_mtime = b.metadata().ok().and_then(|m| m.modified().ok());
        b_mtime.cmp(&a_mtime)
    });

    entries
        .into_iter()
        .map(|e| e.path().to_string_lossy().to_string())
        .collect()
}

#[must_use]
pub fn read_current_top_level_task(plan_path: &str) -> Option<TopLevelTask> {
    let content = fs::read_to_string(plan_path).ok()?;
    let mut current_section: Option<TaskSection> = None;

    for line in content.lines() {
        let trimmed = line.trim_start();

        if trimmed.starts_with('#') {
            let heading_text = trimmed.trim_start_matches('#').trim();
            if heading_text.contains("TODO") {
                current_section = Some(TaskSection::Todo);
            } else if heading_text.contains("Final Verification Wave") {
                current_section = Some(TaskSection::FinalWave);
            }
            continue;
        }

        if current_section.is_none() {
            continue;
        }

        let is_unchecked = trimmed.starts_with("- [ ]") || trimmed.starts_with("* [ ]");
        if !is_unchecked {
            continue;
        }

        let task_text = trimmed
            .strip_prefix("- [ ]")
            .or_else(|| trimmed.strip_prefix("* [ ]"))
            .map_or(trimmed, str::trim);

        let (label, title) = parse_task_label(
            task_text,
            current_section.as_ref().expect("section guard on line 88"),
        );
        let section = current_section.expect("section guard on line 88");

        let key = match &section {
            TaskSection::Todo => format!("todo:{label}"),
            TaskSection::FinalWave => format!("final-wave:{label}"),
        };

        return Some(TopLevelTask {
            key,
            section,
            label,
            title,
        });
    }

    None
}

fn parse_task_label(text: &str, section: &TaskSection) -> (String, String) {
    let expected_prefix = match section {
        TaskSection::Todo => "todo_num",
        TaskSection::FinalWave => "wave_num",
    };

    let parts: Vec<&str> = text.splitn(2, '.').collect();
    if parts.len() == 2 {
        let potential_label = parts[0].trim();
        let title = parts[1].trim().to_string();

        let is_valid = match expected_prefix {
            "todo_num" => potential_label.parse::<u32>().is_ok(),
            "wave_num" => {
                potential_label.starts_with('F') && potential_label[1..].parse::<u32>().is_ok()
            }
            _ => false,
        };

        if is_valid {
            return (potential_label.to_string(), title);
        }
    }

    (String::new(), text.trim().to_string())
}
