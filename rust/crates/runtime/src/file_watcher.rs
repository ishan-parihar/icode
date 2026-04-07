use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as sync_mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use glob::Pattern;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::{Config, Event as NotifyEvent, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::event_bus::{Event, EventBus};

#[derive(Debug)]
pub enum FileWatcherError {
    NotifyError(String),
    IoError(std::io::Error),
}

impl std::fmt::Display for FileWatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotifyError(msg) => write!(f, "file watcher notify error: {msg}"),
            Self::IoError(e) => write!(f, "file watcher io error: {e}"),
        }
    }
}

impl std::error::Error for FileWatcherError {}

impl From<std::io::Error> for FileWatcherError {
    fn from(e: std::io::Error) -> Self {
        Self::IoError(e)
    }
}

pub struct FileWatcherConfig {
    pub watch_paths: Vec<PathBuf>,
    pub exclude_patterns: Vec<String>,
    pub debounce_ms: u64,
    pub respect_gitignore: bool,
}

impl Default for FileWatcherConfig {
    fn default() -> Self {
        Self {
            watch_paths: vec![PathBuf::from(".")],
            exclude_patterns: vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
            ],
            debounce_ms: 300,
            respect_gitignore: true,
        }
    }
}

struct PendingEvents {
    events: HashMap<PathBuf, (EventKind, Instant)>,
    debounce: Duration,
}

impl PendingEvents {
    fn new(debounce: Duration) -> Self {
        Self {
            events: HashMap::new(),
            debounce,
        }
    }

    fn insert(&mut self, path: PathBuf, kind: EventKind) {
        self.events.insert(path, (kind, Instant::now()));
    }

    fn drain_expired(&mut self) -> Vec<(PathBuf, EventKind)> {
        let now = Instant::now();
        let mut expired = Vec::new();

        self.events.retain(|path, (kind, timestamp)| {
            if now.duration_since(*timestamp) >= self.debounce {
                expired.push((path.clone(), *kind));
                false
            } else {
                true
            }
        });

        expired
    }

    fn drain_all(&mut self) -> Vec<(PathBuf, EventKind)> {
        self.events
            .drain()
            .map(|(path, (kind, _))| (path, kind))
            .collect()
    }
}

fn build_gitignore_matcher(paths: &[PathBuf]) -> Option<Gitignore> {
    let mut builder = GitignoreBuilder::new(paths.first()?);
    for path in paths {
        let gitignore_file = path.join(".gitignore");
        if gitignore_file.exists() {
            builder.add(&gitignore_file);
        }
    }
    builder.build().ok()
}

fn is_excluded_by_patterns(path: &Path, patterns: &[Pattern]) -> bool {
    for component in path.components() {
        if let Some(name) = component.as_os_str().to_str() {
            for pattern in patterns {
                if pattern.matches(name) {
                    return true;
                }
            }
        }
    }
    false
}

fn is_excluded_by_gitignore(path: &Path, gitignore: Option<&Gitignore>) -> bool {
    let Some(gi) = gitignore else {
        return false;
    };
    matches!(gi.matched(path, path.is_dir()), ignore::Match::Ignore(_))
}

fn event_kind_to_string(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Create(_) => "create",
        EventKind::Modify(_) => "modify",
        EventKind::Remove(_) => "delete",
        EventKind::Access(_) => "access",
        EventKind::Other => "other",
        EventKind::Any => "any",
    }
}

fn is_interesting_event(kind: EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

pub struct FileWatcher {
    watcher: Option<RecommendedWatcher>,
    config: FileWatcherConfig,
    event_bus: Arc<EventBus>,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
    event_rx: Option<sync_mpsc::Receiver<Result<NotifyEvent, notify::Error>>>,
}

fn emit_file_changed(event_bus: &EventBus, path: &Path, kind: EventKind) {
    let kind_str = event_kind_to_string(kind);
    event_bus.publish(Event::FileChanged {
        path: path.to_string_lossy().to_string(),
        kind: kind_str.to_string(),
    });
}

#[allow(clippy::needless_pass_by_value)]
fn process_events(
    rx: sync_mpsc::Receiver<Result<NotifyEvent, notify::Error>>,
    stop_rx: sync_mpsc::Receiver<()>,
    running: Arc<AtomicBool>,
    event_bus: Arc<EventBus>,
    gitignore: Option<Gitignore>,
    exclude_patterns: Vec<Pattern>,
    debounce: Duration,
) {
    let mut pending = PendingEvents::new(debounce);
    let flush_interval = std::cmp::max(debounce / 2, Duration::from_millis(50));

    loop {
        match rx.recv_timeout(flush_interval) {
            Ok(Ok(event)) => {
                for path in &event.paths {
                    if is_interesting_event(event.kind)
                        && !is_excluded_by_patterns(path, &exclude_patterns)
                        && !is_excluded_by_gitignore(path, gitignore.as_ref())
                    {
                        pending.insert(path.clone(), event.kind);
                    }
                }
                for (path, kind) in pending.drain_expired() {
                    emit_file_changed(&event_bus, &path, kind);
                }
            }
            Ok(Err(e)) => {
                eprintln!("file_watcher: notify error: {e}");
            }
            Err(sync_mpsc::RecvTimeoutError::Timeout) => {
                for (path, kind) in pending.drain_expired() {
                    emit_file_changed(&event_bus, &path, kind);
                }
            }
            Err(sync_mpsc::RecvTimeoutError::Disconnected) => {
                for (path, kind) in pending.drain_all() {
                    emit_file_changed(&event_bus, &path, kind);
                }
                break;
            }
        }

        if !running.load(Ordering::SeqCst) || stop_rx.try_recv().is_ok() {
            for (path, kind) in pending.drain_all() {
                emit_file_changed(&event_bus, &path, kind);
            }
            break;
        }
    }
}

impl FileWatcher {
    pub fn new(
        config: FileWatcherConfig,
        event_bus: Arc<EventBus>,
    ) -> Result<Self, FileWatcherError> {
        let (tx, rx) = sync_mpsc::channel::<Result<NotifyEvent, notify::Error>>();

        let watcher: RecommendedWatcher = RecommendedWatcher::new(tx, Config::default())
            .map_err(|e| FileWatcherError::NotifyError(format!("failed to create watcher: {e}")))?;

        Ok(Self {
            watcher: Some(watcher),
            config,
            event_bus,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
            event_rx: Some(rx),
        })
    }

    pub fn start(&mut self) -> Result<(), FileWatcherError> {
        let watcher = self
            .watcher
            .as_mut()
            .ok_or_else(|| FileWatcherError::NotifyError("watcher already consumed".to_string()))?;

        for path in &self.config.watch_paths {
            let canonical = path.canonicalize().map_err(|e| {
                FileWatcherError::IoError(std::io::Error::new(
                    e.kind(),
                    format!("cannot canonicalize watch path {}: {e}", path.display()),
                ))
            })?;
            watcher
                .watch(&canonical, RecursiveMode::Recursive)
                .map_err(|e| {
                    FileWatcherError::NotifyError(format!(
                        "failed to watch {}: {e}",
                        canonical.display()
                    ))
                })?;
        }

        let gitignore = if self.config.respect_gitignore {
            build_gitignore_matcher(&self.config.watch_paths)
        } else {
            None
        };

        let exclude_patterns: Vec<Pattern> = self
            .config
            .exclude_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        let debounce = Duration::from_millis(self.config.debounce_ms);
        let running = Arc::clone(&self.running);
        let event_bus = Arc::clone(&self.event_bus);
        let rx = self.event_rx.take().ok_or_else(|| {
            FileWatcherError::NotifyError("event receiver already consumed".to_string())
        })?;
        let (stop_tx, stop_rx) = sync_mpsc::channel::<()>();

        let handle = thread::spawn(move || {
            process_events(
                rx,
                stop_rx,
                running,
                event_bus,
                gitignore,
                exclude_patterns,
                debounce,
            );
        });

        self.running.store(true, Ordering::SeqCst);
        self.handle = Some(handle);
        let _ = stop_tx;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }

        if let Some(watcher) = &mut self.watcher {
            for path in &self.config.watch_paths {
                if let Ok(canonical) = path.canonicalize() {
                    let _ = watcher.unwatch(&canonical);
                }
            }
        }
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should move forward")
            .as_nanos();
        std::env::temp_dir().join(format!("icode-filewatcher-{name}-{unique}"))
    }

    #[test]
    fn config_defaults_are_correct() {
        let config = FileWatcherConfig::default();

        assert_eq!(config.watch_paths, vec![PathBuf::from(".")]);
        assert_eq!(
            config.exclude_patterns,
            vec![".git", "target", "node_modules"]
        );
        assert_eq!(config.debounce_ms, 300);
        assert!(config.respect_gitignore);
    }

    #[test]
    fn exclude_pattern_matching_filters_correctly() {
        let patterns: Vec<Pattern> = [".git", "target", "node_modules"]
            .iter()
            .map(|p| Pattern::new(p).expect("pattern should parse"))
            .collect();

        assert!(is_excluded_by_patterns(
            Path::new("/project/.git"),
            &patterns
        ));
        assert!(is_excluded_by_patterns(
            Path::new("/project/target"),
            &patterns
        ));
        assert!(is_excluded_by_patterns(
            Path::new("/project/node_modules"),
            &patterns
        ));
        assert!(is_excluded_by_patterns(Path::new(".git"), &patterns));
        assert!(is_excluded_by_patterns(Path::new("target"), &patterns));

        assert!(!is_excluded_by_patterns(
            Path::new("/project/src/main.rs"),
            &patterns
        ));
        assert!(!is_excluded_by_patterns(
            Path::new("/project/Cargo.toml"),
            &patterns
        ));
        assert!(!is_excluded_by_patterns(
            Path::new("/project/src/target_file.rs"),
            &patterns
        ));
    }

    #[test]
    fn gitignore_filtering_respects_gitignore_file() {
        let dir = temp_dir("gitignore-filter");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        std::fs::write(dir.join(".gitignore"), "ignored_dir/\n*.log\n")
            .expect(".gitignore should write");

        std::fs::create_dir_all(dir.join("ignored_dir")).expect("ignored_dir should create");
        std::fs::create_dir_all(dir.join("visible_dir")).expect("visible_dir should create");
        std::fs::write(dir.join("visible.txt"), "visible").expect("visible.txt should write");
        std::fs::write(dir.join("debug.log"), "log content").expect("debug.log should write");

        let gitignore = build_gitignore_matcher(std::slice::from_ref(&dir));

        assert!(is_excluded_by_gitignore(
            &dir.join("ignored_dir"),
            gitignore.as_ref()
        ));
        assert!(is_excluded_by_gitignore(
            &dir.join("debug.log"),
            gitignore.as_ref()
        ));

        assert!(!is_excluded_by_gitignore(
            &dir.join("visible.txt"),
            gitignore.as_ref()
        ));
        assert!(!is_excluded_by_gitignore(
            &dir.join("visible_dir"),
            gitignore.as_ref()
        ));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn debouncing_merges_rapid_events() {
        let debounce = Duration::from_millis(100);
        let mut pending = PendingEvents::new(debounce);

        let path1 = PathBuf::from("/test/file.rs");

        pending.insert(
            path1.clone(),
            EventKind::Modify(notify::event::ModifyKind::Any),
        );
        thread::sleep(Duration::from_millis(10));
        pending.insert(
            path1.clone(),
            EventKind::Modify(notify::event::ModifyKind::Any),
        );
        pending.insert(
            path1.clone(),
            EventKind::Create(notify::event::CreateKind::Any),
        );

        assert_eq!(pending.events.len(), 1);

        thread::sleep(Duration::from_millis(150));

        let expired = pending.drain_expired();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].0, path1);
        assert!(matches!(expired[0].1, EventKind::Create(_)));
    }

    #[test]
    fn event_kind_mapping_converts_correctly() {
        assert_eq!(
            event_kind_to_string(EventKind::Create(notify::event::CreateKind::Any)),
            "create"
        );
        assert_eq!(
            event_kind_to_string(EventKind::Create(notify::event::CreateKind::File)),
            "create"
        );
        assert_eq!(
            event_kind_to_string(EventKind::Modify(notify::event::ModifyKind::Any)),
            "modify"
        );
        assert_eq!(
            event_kind_to_string(EventKind::Modify(notify::event::ModifyKind::Data(
                notify::event::DataChange::Content
            ))),
            "modify"
        );
        assert_eq!(
            event_kind_to_string(EventKind::Remove(notify::event::RemoveKind::Any)),
            "delete"
        );
        assert_eq!(
            event_kind_to_string(EventKind::Remove(notify::event::RemoveKind::File)),
            "delete"
        );
        assert_eq!(
            event_kind_to_string(EventKind::Access(notify::event::AccessKind::Any)),
            "access"
        );
        assert_eq!(event_kind_to_string(EventKind::Any), "any");
        assert_eq!(event_kind_to_string(EventKind::Other), "other");
    }

    #[test]
    fn is_interesting_event_filters_correctly() {
        assert!(is_interesting_event(EventKind::Create(
            notify::event::CreateKind::Any
        )));
        assert!(is_interesting_event(EventKind::Modify(
            notify::event::ModifyKind::Any
        )));
        assert!(is_interesting_event(EventKind::Remove(
            notify::event::RemoveKind::Any
        )));
        assert!(!is_interesting_event(EventKind::Access(
            notify::event::AccessKind::Any
        )));
        assert!(!is_interesting_event(EventKind::Any));
        assert!(!is_interesting_event(EventKind::Other));
    }

    #[test]
    fn pending_events_drain_all_returns_all_entries() {
        let mut pending = PendingEvents::new(Duration::from_millis(1000));

        pending.insert(
            PathBuf::from("/a.rs"),
            EventKind::Create(notify::event::CreateKind::Any),
        );
        pending.insert(
            PathBuf::from("/b.rs"),
            EventKind::Modify(notify::event::ModifyKind::Any),
        );

        let all = pending.drain_all();
        assert_eq!(all.len(), 2);
        assert!(pending.events.is_empty());
    }

    #[cfg_attr(not(feature = "watcher-tests"), ignore = "requires file watcher support")]
    #[test]
    fn file_watcher_new_and_stop_works() {
        let dir = temp_dir("new-stop");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        let config = FileWatcherConfig {
            watch_paths: vec![dir.clone()],
            exclude_patterns: vec![".git".to_string()],
            debounce_ms: 100,
            respect_gitignore: false,
        };

        let event_bus = Arc::new(EventBus::new(16));
        let mut watcher = FileWatcher::new(config, event_bus).expect("watcher should be created");

        watcher.start().expect("start should succeed");
        thread::sleep(Duration::from_millis(50));
        watcher.stop();

        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg_attr(not(feature = "watcher-tests"), ignore = "requires file watcher support")]
    #[test]
    fn file_watcher_emits_events_on_file_change() {
        let dir = temp_dir("emit-events");
        std::fs::create_dir_all(&dir).expect("test dir should create");

        let config = FileWatcherConfig {
            watch_paths: vec![dir.clone()],
            exclude_patterns: vec![],
            debounce_ms: 50,
            respect_gitignore: false,
        };

        let event_bus = Arc::new(EventBus::new(64));
        let mut rx = event_bus.subscribe();
        let mut watcher =
            FileWatcher::new(config, Arc::clone(&event_bus)).expect("watcher should be created");

        watcher.start().expect("start should succeed");
        thread::sleep(Duration::from_millis(100));

        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, "hello").expect("file should write");

        thread::sleep(Duration::from_millis(500));

        let mut found_event = false;
        let mut received_events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let Event::FileChanged { path, kind } = event {
                received_events.push((path.clone(), kind.clone()));
                if path.contains("test.txt") {
                    found_event = true;
                }
            }
        }

        assert!(
            found_event,
            "should have received FileChanged event for test.txt. Received: {received_events:?}"
        );

        watcher.stop();
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg_attr(not(feature = "watcher-tests"), ignore = "requires file watcher support")]
    #[test]
    fn file_watcher_excludes_configured_patterns() {
        let dir = temp_dir("exclude-patterns");
        std::fs::create_dir_all(dir.join("target")).expect("target dir should create");
        std::fs::create_dir_all(dir.join("src")).expect("src dir should create");

        let config = FileWatcherConfig {
            watch_paths: vec![dir.clone()],
            exclude_patterns: vec!["target".to_string()],
            debounce_ms: 50,
            respect_gitignore: false,
        };

        let event_bus = Arc::new(EventBus::new(64));
        let mut rx = event_bus.subscribe();
        let mut watcher =
            FileWatcher::new(config, Arc::clone(&event_bus)).expect("watcher should be created");

        watcher.start().expect("start should succeed");
        thread::sleep(Duration::from_millis(100));

        std::fs::write(dir.join("target").join("build.rs"), "// build")
            .expect("target file should write");
        std::fs::write(dir.join("src").join("main.rs"), "// main").expect("src file should write");

        thread::sleep(Duration::from_millis(500));

        let mut target_events = 0;
        let mut src_events = 0;
        while let Ok(event) = rx.try_recv() {
            if let Event::FileChanged { path, .. } = event {
                if path.contains("target") {
                    target_events += 1;
                }
                if path.contains("src") {
                    src_events += 1;
                }
            }
        }

        assert_eq!(
            target_events, 0,
            "target directory events should be excluded"
        );
        assert!(src_events > 0, "src directory events should be present");

        watcher.stop();
        std::fs::remove_dir_all(&dir).ok();
    }
}
