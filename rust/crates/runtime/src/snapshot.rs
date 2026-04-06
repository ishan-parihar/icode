use std::io::Write;
use std::io::Write;
use std::io::Write;
use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileStatus { Added, Modified, Deleted, Unchanged }
#[derive(Debug, Clone)]
pub struct FileDiff { pub path: PathBuf, pub status: FileStatus }
#[derive(Debug)]
pub enum SnapshotError { IoError(std::io::Error) }
impl std::fmt::Display for SnapshotError { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { match self { Self::IoError(e) => write!(f, "{e}") } } }
impl From<std::io::Error> for SnapshotError { fn from(e: std::io::Error) -> Self { Self::IoError(e) } }
#[derive(Debug, Clone)]
pub struct SnapshotRecord { pub files: Vec<FileDiff> }
#[derive(Debug, Clone)]
pub struct SnapshotManager;
impl SnapshotManager { pub fn new() -> Self { Self } pub fn diff(&self) -> Result<Vec<FileDiff>, SnapshotError> { Ok(Vec::new()) } pub fn save(&self, _: &str) -> Result<SnapshotRecord, SnapshotError> { Ok(SnapshotRecord { files: Vec::new() }) } }
