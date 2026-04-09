use std::cmp::Ordering;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, JsonSchema)]
pub struct ListDirectoryInput {
    pub path: String,
    #[serde(default)]
    pub depth: Option<u32>,
    #[serde(default)]
    pub show_hidden: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub name: String,
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(rename = "symlink_target", skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListDirectoryOutput {
    pub entries: Vec<DirectoryEntry>,
    pub truncated: bool,
}

const MAX_DEPTH: u32 = 3;
const DEFAULT_DEPTH: u32 = 1;
const MAX_ENTRIES: usize = 10_000;

fn resolve_path(path: &str) -> io::Result<PathBuf> {
    let candidate = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        std::env::current_dir()?.join(path)
    };
    candidate.canonicalize()
}

fn validate_workspace_boundary(resolved: &Path, workspace_root: &Path) -> io::Result<()> {
    if !resolved.starts_with(workspace_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "path {} escapes workspace boundary {}",
                resolved.display(),
                workspace_root.display()
            ),
        ));
    }
    Ok(())
}

fn entry_type_string(metadata: &fs::Metadata) -> String {
    if metadata.is_symlink() {
        String::from("symlink")
    } else if metadata.is_dir() {
        String::from("directory")
    } else {
        String::from("file")
    }
}

fn format_time(time: std::time::SystemTime) -> Option<String> {
    time.duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| {
            let secs = duration.as_secs().cast_signed();
            chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
        })
}

fn build_entry(entry_path: &Path, base_path: &Path, metadata: &fs::Metadata) -> DirectoryEntry {
    let name = entry_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let relative_path = entry_path
        .strip_prefix(base_path)
        .unwrap_or(entry_path)
        .to_string_lossy()
        .into_owned();

    let kind = entry_type_string(metadata);

    let size = if metadata.is_file() || metadata.is_symlink() {
        Some(metadata.len())
    } else {
        None
    };

    let modified = metadata.modified().ok().and_then(format_time);

    let symlink_target = if metadata.is_symlink() {
        fs::read_link(entry_path)
            .ok()
            .map(|target| target.to_string_lossy().into_owned())
    } else {
        None
    };

    DirectoryEntry {
        name,
        path: relative_path,
        kind,
        size,
        modified,
        symlink_target,
    }
}

/// Compare two `DirectoryEntry` values for sorting:
/// directories first, then files and symlinks, alphabetically within each group.
fn compare_entries(a: &DirectoryEntry, b: &DirectoryEntry) -> Ordering {
    let a_is_dir = a.kind == "directory";
    let b_is_dir = b.kind == "directory";

    match (a_is_dir, b_is_dir) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => a.name.cmp(&b.name),
    }
}

pub fn list_directory(
    input: &ListDirectoryInput,
    workspace_root: Option<&Path>,
) -> io::Result<ListDirectoryOutput> {
    let resolved = resolve_path(&input.path)?;

    // Validate that the path is a directory
    if !resolved.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            format!("{} is not a directory", resolved.display()),
        ));
    }

    // Validate workspace boundary if workspace root is provided
    if let Some(root) = workspace_root {
        let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        validate_workspace_boundary(&resolved, &canonical_root)?;
    }

    let depth = input.depth.unwrap_or(DEFAULT_DEPTH).clamp(1, MAX_DEPTH);
    let show_hidden = input.show_hidden.unwrap_or(false);

    let mut walk_builder = WalkBuilder::new(&resolved);
    walk_builder
        .max_depth(Some(depth as usize))
        .hidden(!show_hidden)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .follow_links(false);

    let mut entries = Vec::new();
    let mut truncated = false;

    for result in walk_builder.build() {
        let entry = result.map_err(|error| io::Error::other(error.to_string()))?;

        // Skip the root directory itself
        if entry.path() == resolved {
            continue;
        }

        let symlink_metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            io::Error::other(format!(
                "failed to read metadata for {}: {}",
                entry.path().display(),
                error
            ))
        })?;
        let dir_entry = build_entry(entry.path(), &resolved, &symlink_metadata);
        entries.push(dir_entry);

        if entries.len() >= MAX_ENTRIES {
            truncated = true;
            break;
        }
    }

    entries.sort_by(compare_entries);

    Ok(ListDirectoryOutput { entries, truncated })
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{list_directory, ListDirectoryInput};

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!("icode-listdir-{name}-{unique}"))
    }

    fn setup_test_dir() -> std::path::PathBuf {
        let dir = temp_dir("basic");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        // Create files
        std::fs::write(dir.join("file_a.txt"), "content a").expect("file_a should write");
        std::fs::write(dir.join("file_b.rs"), "content b").expect("file_b should write");

        // Create subdirectories
        std::fs::create_dir_all(dir.join("subdir_a")).expect("subdir_a should create");
        std::fs::create_dir_all(dir.join("subdir_b")).expect("subdir_b should create");

        // Create hidden files
        std::fs::write(dir.join(".hidden_file"), "hidden").expect(".hidden_file should write");
        std::fs::create_dir_all(dir.join(".hidden_dir")).expect(".hidden_dir should create");

        dir
    }

    #[test]
    fn lists_immediate_children() {
        let dir = setup_test_dir();

        let output = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: Some(1),
                show_hidden: Some(false),
            },
            None,
        )
        .expect("list_directory should succeed");

        // Should have 4 entries: 2 dirs + 2 files (no hidden)
        assert_eq!(output.entries.len(), 4);

        // Directories should come first, sorted alphabetically
        assert_eq!(output.entries[0].name, "subdir_a");
        assert_eq!(output.entries[0].kind, "directory");
        assert_eq!(output.entries[1].name, "subdir_b");
        assert_eq!(output.entries[1].kind, "directory");

        // Then files, sorted alphabetically
        assert_eq!(output.entries[2].name, "file_a.txt");
        assert_eq!(output.entries[2].kind, "file");
        assert_eq!(output.entries[3].name, "file_b.rs");
        assert_eq!(output.entries[3].kind, "file");

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn includes_hidden_files_when_requested() {
        let dir = setup_test_dir();

        let output = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: Some(1),
                show_hidden: Some(true),
            },
            None,
        )
        .expect("list_directory should succeed");

        // Should have 6 entries: 4 visible + 2 hidden
        assert_eq!(output.entries.len(), 6);

        let names: Vec<&str> = output.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&".hidden_file"));
        assert!(names.contains(&".hidden_dir"));

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respects_depth_limit() {
        let dir = setup_test_dir();

        // Add nested content
        std::fs::write(dir.join("subdir_a").join("nested.txt"), "nested content")
            .expect("nested file should write");

        // Depth 1 should not include nested files
        let output_depth1 = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: Some(1),
                show_hidden: Some(false),
            },
            None,
        )
        .expect("list_directory depth=1 should succeed");

        let names_depth1: Vec<&str> = output_depth1
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(!names_depth1.contains(&"nested.txt"));

        // Depth 2 should include nested files
        let output_depth2 = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: Some(2),
                show_hidden: Some(false),
            },
            None,
        )
        .expect("list_directory depth=2 should succeed");

        let names_depth2: Vec<&str> = output_depth2
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        assert!(names_depth2.contains(&"nested.txt"));

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn respects_gitignore() {
        let dir = temp_dir("gitignore");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        // Create .gitignore
        std::fs::write(dir.join(".gitignore"), "ignored_dir/\nignored.txt\n")
            .expect(".gitignore should write");

        // Create ignored and non-ignored files
        std::fs::write(dir.join("visible.txt"), "visible").expect("visible.txt should write");
        std::fs::write(dir.join("ignored.txt"), "ignored").expect("ignored.txt should write");
        std::fs::create_dir_all(dir.join("ignored_dir")).expect("ignored_dir should create");
        std::fs::create_dir_all(dir.join("visible_dir")).expect("visible_dir should create");

        let output = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: Some(1),
                show_hidden: Some(true), // show_hidden=true but gitignore should still apply
            },
            None,
        )
        .expect("list_directory should succeed");

        let names: Vec<&str> = output.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"visible.txt"));
        assert!(names.contains(&"visible_dir"));
        assert!(!names.contains(&"ignored.txt"));
        assert!(!names.contains(&"ignored_dir"));
        // .gitignore itself should be included (it's not ignored by its own patterns)
        assert!(names.contains(&".gitignore"));

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn returns_error_for_nonexistent_path() {
        let result = list_directory(
            &ListDirectoryInput {
                path: String::from("/nonexistent/path/that/does/not/exist"),
                depth: Some(1),
                show_hidden: Some(false),
            },
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn returns_error_for_file_instead_of_directory() {
        let dir = temp_dir("not-a-dir");
        std::fs::create_dir_all(&dir).expect("test dir should create");
        let file = dir.join("a_file.txt");
        std::fs::write(&file, "content").expect("file should write");

        let result = list_directory(
            &ListDirectoryInput {
                path: file.to_string_lossy().into_owned(),
                depth: Some(1),
                show_hidden: Some(false),
            },
            None,
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not a directory"));

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn returns_error_for_path_outside_workspace() {
        let workspace = temp_dir("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace should create");

        let outside = temp_dir("outside");
        std::fs::create_dir_all(&outside).expect("outside should create");

        let result = list_directory(
            &ListDirectoryInput {
                path: outside.to_string_lossy().into_owned(),
                depth: Some(1),
                show_hidden: Some(false),
            },
            Some(&workspace),
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("escapes workspace boundary"));

        // Clean up
        std::fs::remove_dir_all(&workspace).ok();
        std::fs::remove_dir_all(&outside).ok();
    }

    #[test]
    fn reports_symlinks_correctly() {
        let dir = temp_dir("symlinks");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        let target = dir.join("target.txt");
        std::fs::write(&target, "target content").expect("target should write");

        #[cfg(unix)]
        {
            let link = dir.join("my_link");
            std::os::unix::fs::symlink(&target, &link).expect("symlink should create");

            let output = list_directory(
                &ListDirectoryInput {
                    path: dir.to_string_lossy().into_owned(),
                    depth: Some(1),
                    show_hidden: Some(false),
                },
                None,
            )
            .expect("list_directory should succeed");

            let link_entry = output
                .entries
                .iter()
                .find(|e| e.name == "my_link")
                .expect("symlink entry should exist");
            assert_eq!(link_entry.kind, "symlink");
            assert!(link_entry.symlink_target.is_some());
        }

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn clamps_depth_to_max() {
        let dir = setup_test_dir();

        // Requesting depth=10 should be clamped to 3
        let output = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: Some(10),
                show_hidden: Some(false),
            },
            None,
        )
        .expect("list_directory should succeed");

        // Should still work (not crash), just limited to depth 3
        assert!(!output.entries.is_empty());

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn uses_default_depth_when_not_specified() {
        let dir = setup_test_dir();

        let output = list_directory(
            &ListDirectoryInput {
                path: dir.to_string_lossy().into_owned(),
                depth: None,
                show_hidden: Some(false),
            },
            None,
        )
        .expect("list_directory should succeed");

        // Default depth=1, so only immediate children
        assert_eq!(output.entries.len(), 4);

        // Clean up
        std::fs::remove_dir_all(&dir).ok();
    }
}
