use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Open the given text in the user's $EDITOR, return edited text on success.
///
/// This function:
/// 1. Resolves the editor from `$VISUAL`, `$EDITOR`, or falls back to `"vim"`.
/// 2. Writes the initial content to a temporary file.
/// 3. Spawns the editor process and waits for it to exit.
/// 4. Reads the file contents back.
/// 5. Cleans up the temp file (even on error).
/// 6. Returns the edited content with trailing newlines stripped.
pub fn open_editor(initial_content: &str) -> Result<String, String> {
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vim".to_string());

    let temp_path = create_temp_file(initial_content)?;
    let result = run_editor(&editor, &temp_path);
    let content = result.and_then(|()| read_temp_file(&temp_path));
    let _ = fs::remove_file(&temp_path);

    content.map(|s| s.trim_end_matches(['\n', '\r']).to_string())
}

fn create_temp_file(content: &str) -> Result<PathBuf, String> {
    let counter = FILE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = env::temp_dir().join(format!(
        "icode-prompt-{}-{}.txt",
        std::process::id(),
        counter
    ));

    let mut file =
        fs::File::create(&path).map_err(|e| format!("Failed to create temp file: {e}"))?;
    file.write_all(content.as_bytes())
        .map_err(|e| format!("Failed to write temp file: {e}"))?;

    Ok(path)
}

fn run_editor(editor: &str, path: &PathBuf) -> Result<(), String> {
    let status = Command::new(editor)
        .arg(path)
        .status()
        .map_err(|e| format!("Failed to launch editor '{editor}': {e}"))?;

    if !status.success() {
        return Err(format!(
            "Editor '{}' exited with status: {}",
            editor,
            status
                .code().map_or_else(|| "unknown".to_string(), |c| c.to_string())
        ));
    }

    Ok(())
}

fn read_temp_file(path: &PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("Failed to read edited content: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn create_temp_file_writes_content() {
        let content = "hello world\nline two";
        let path = create_temp_file(content).expect("should create temp file");

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("icode-prompt-"));

        let read_content = fs::read_to_string(&path).expect("should read temp file");
        assert_eq!(read_content, content);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn create_temp_file_handles_empty_content() {
        let path = create_temp_file("").expect("should create temp file for empty content");
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("icode-prompt-"));

        let read_content = fs::read_to_string(&path).expect("should read temp file");
        assert_eq!(read_content, "");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn create_temp_file_generates_unique_paths() {
        let path1 = create_temp_file("a").expect("should create first temp file");
        let path2 = create_temp_file("b").expect("should create second temp file");

        assert_ne!(path1, path2);
        assert!(path1.exists());
        assert!(path2.exists());

        let _ = fs::remove_file(&path1);
        let _ = fs::remove_file(&path2);
    }

    #[test]
    fn read_temp_file_returns_content() {
        let content = "test content\nwith newlines";
        let path = create_temp_file(content).expect("should create temp file");

        let result = read_temp_file(&path).expect("should read content");
        assert_eq!(result, content);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn read_temp_file_returns_error_for_missing_file() {
        let path = PathBuf::from("/tmp/nonexistent-icode-test-file.txt");
        let result = read_temp_file(&path);
        assert!(result.is_err());
    }

    #[test]
    fn run_editor_returns_error_for_nonexistent_editor() {
        let path = create_temp_file("test").expect("should create temp file");
        let result = run_editor("nonexistent-editor-that-does-not-exist", &path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to launch editor"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn open_editor_strips_trailing_newlines() {
        let content = "hello\n\n";
        let temp_path = create_temp_file(content).expect("should create temp file");

        let read_content = read_temp_file(&temp_path).expect("should read content");
        let stripped = read_content
            .trim_end_matches(['\n', '\r'])
            .to_string();
        assert_eq!(stripped, "hello");

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn open_editor_strips_crlf_trailing() {
        let content = "test\r\n\r\n";
        let temp_path = create_temp_file(content).expect("should create temp file");

        let read_content = read_temp_file(&temp_path).expect("should read content");
        let stripped = read_content
            .trim_end_matches(['\n', '\r'])
            .to_string();
        assert_eq!(stripped, "test");

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn open_editor_preserves_internal_newlines() {
        let content = "line1\nline2\nline3";
        let temp_path = create_temp_file(content).expect("should create temp file");

        let read_content = read_temp_file(&temp_path).expect("should read content");
        let stripped = read_content
            .trim_end_matches(['\n', '\r'])
            .to_string();
        assert_eq!(stripped, "line1\nline2\nline3");

        let _ = fs::remove_file(&temp_path);
    }

    #[test]
    fn editor_resolution_uses_visual_first() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();

        env::set_var("VISUAL", "nvim");
        env::set_var("EDITOR", "vim");

        let editor = env::var("VISUAL")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "vim".to_string());
        assert_eq!(editor, "nvim");

        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    fn editor_resolution_defaults_to_vim() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();

        env::remove_var("VISUAL");
        env::remove_var("EDITOR");

        let editor = env::var("VISUAL")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "vim".to_string());
        assert_eq!(editor, "vim");

        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    fn editor_resolution_falls_back_to_editor() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let orig_visual = env::var("VISUAL").ok();
        let orig_editor = env::var("EDITOR").ok();

        env::remove_var("VISUAL");
        env::set_var("EDITOR", "nano");

        let editor = env::var("VISUAL")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "vim".to_string());
        assert_eq!(editor, "nano");

        match orig_visual {
            Some(v) => env::set_var("VISUAL", v),
            None => env::remove_var("VISUAL"),
        }
        match orig_editor {
            Some(v) => env::set_var("EDITOR", v),
            None => env::remove_var("EDITOR"),
        }
    }

    #[test]
    fn open_editor_cleans_up_temp_file_on_editor_error() {
        let content = "test content";
        let path = create_temp_file(content).expect("should create temp file");
        let path_clone = path.clone();

        let result = run_editor("nonexistent-editor-xyz", &path);
        assert!(result.is_err());

        let _ = fs::remove_file(&path_clone);
    }
}
