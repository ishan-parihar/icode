//! Self-contained trust resolution for repository and worktree paths.
//!
//! Evaluates a `(repo_path, worktree_path)` pair against a [`TrustConfig`]
//! of allowlisted and denied paths, returning a [`TrustDecision`] with the
//! chosen [`TrustPolicy`] and a log of [`TrustEvent`]s.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustPolicy {
    AutoTrust,
    RequireApproval,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustEvent {
    TrustRequired { repo: String, worktree: String },
    TrustResolved { repo: String, policy: TrustPolicy },
    TrustDenied { repo: String, reason: String },
}

#[derive(Debug, Clone, Default)]
pub struct TrustConfig {
    allowlisted: Vec<PathBuf>,
    denied: Vec<PathBuf>,
}

impl TrustConfig {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_allowlisted(mut self, path: impl Into<PathBuf>) -> Self {
        self.allowlisted.push(path.into());
        self
    }

    #[must_use]
    pub fn with_denied(mut self, path: impl Into<PathBuf>) -> Self {
        self.denied.push(path.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustDecision {
    pub policy: TrustPolicy,
    pub events: Vec<TrustEvent>,
}

#[derive(Debug, Clone)]
pub struct TrustResolver {
    config: TrustConfig,
}

impl TrustResolver {
    #[must_use]
    pub fn new(config: TrustConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn resolve_trust(&self, repo_path: &str, worktree_path: &str) -> TrustDecision {
        let mut events = Vec::new();

        events.push(TrustEvent::TrustRequired {
            repo: repo_path.to_owned(),
            worktree: worktree_path.to_owned(),
        });

        if self
            .config
            .denied
            .iter()
            .any(|root| path_matches(repo_path, root) || path_matches(worktree_path, root))
        {
            let reason = format!("repository path matches deny list: {repo_path}");
            events.push(TrustEvent::TrustDenied {
                repo: repo_path.to_owned(),
                reason,
            });
            return TrustDecision {
                policy: TrustPolicy::Deny,
                events,
            };
        }

        if self
            .config
            .allowlisted
            .iter()
            .any(|root| path_matches(repo_path, root) || path_matches(worktree_path, root))
        {
            events.push(TrustEvent::TrustResolved {
                repo: repo_path.to_owned(),
                policy: TrustPolicy::AutoTrust,
            });
            return TrustDecision {
                policy: TrustPolicy::AutoTrust,
                events,
            };
        }

        TrustDecision {
            policy: TrustPolicy::RequireApproval,
            events,
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn path_matches(candidate: &str, root: &Path) -> bool {
    let candidate = normalize_path(Path::new(candidate));
    let root = normalize_path(root);
    candidate == root || candidate.starts_with(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlisted_repo_auto_trusts_and_records_events() {
        // Given: a resolver whose allowlist contains /tmp/trusted
        let config = TrustConfig::new().with_allowlisted("/tmp/trusted");
        let resolver = TrustResolver::new(config);

        // When: we resolve trust for a repo under the allowlisted root
        let decision =
            resolver.resolve_trust("/tmp/trusted/repo-a", "/tmp/trusted/repo-a/worktree");

        // Then: the policy is AutoTrust
        assert_eq!(decision.policy, TrustPolicy::AutoTrust);

        // And: both TrustRequired and TrustResolved events are recorded
        assert!(decision.events.iter().any(|e| matches!(
            e,
            TrustEvent::TrustRequired { repo, worktree }
                if repo == "/tmp/trusted/repo-a"
                    && worktree == "/tmp/trusted/repo-a/worktree"
        )));
        assert!(decision.events.iter().any(|e| matches!(
            e,
            TrustEvent::TrustResolved { policy, .. }
                if *policy == TrustPolicy::AutoTrust
        )));
    }

    #[test]
    fn unknown_repo_requires_approval_and_remains_gated() {
        // Given: a resolver with no matching paths for the tested repo
        let config = TrustConfig::new().with_allowlisted("/tmp/other");
        let resolver = TrustResolver::new(config);

        // When: we resolve trust for an unknown repo
        let decision =
            resolver.resolve_trust("/tmp/unknown/repo-b", "/tmp/unknown/repo-b/worktree");

        // Then: the policy is RequireApproval
        assert_eq!(decision.policy, TrustPolicy::RequireApproval);

        // And: only the TrustRequired event is recorded (no resolution)
        assert_eq!(decision.events.len(), 1);
        assert!(matches!(
            &decision.events[0],
            TrustEvent::TrustRequired { .. }
        ));
    }

    #[test]
    fn denied_repo_blocks_and_records_denial_events() {
        // Given: a resolver whose deny list contains /tmp/blocked
        let config = TrustConfig::new().with_denied("/tmp/blocked");
        let resolver = TrustResolver::new(config);

        // When: we resolve trust for a repo under the denied root
        let decision =
            resolver.resolve_trust("/tmp/blocked/repo-c", "/tmp/blocked/repo-c/worktree");

        // Then: the policy is Deny
        assert_eq!(decision.policy, TrustPolicy::Deny);

        // And: both TrustRequired and TrustDenied events are recorded
        assert!(decision
            .events
            .iter()
            .any(|e| matches!(e, TrustEvent::TrustRequired { .. })));
        assert!(decision.events.iter().any(|e| matches!(
            e,
            TrustEvent::TrustDenied { reason, .. }
                if reason.contains("deny list")
        )));
    }

    #[test]
    fn denied_takes_precedence_over_allowlisted() {
        // Given: a resolver where the same root appears in both lists
        let config = TrustConfig::new()
            .with_allowlisted("/tmp/contested")
            .with_denied("/tmp/contested");
        let resolver = TrustResolver::new(config);

        // When: we resolve trust for a repo under the contested root
        let decision =
            resolver.resolve_trust("/tmp/contested/repo-d", "/tmp/contested/repo-d/worktree");

        // Then: deny takes precedence — policy is Deny
        assert_eq!(decision.policy, TrustPolicy::Deny);

        // And: TrustDenied is recorded, but TrustResolved is not
        assert!(decision
            .events
            .iter()
            .any(|e| matches!(e, TrustEvent::TrustDenied { .. })));
        assert!(!decision
            .events
            .iter()
            .any(|e| matches!(e, TrustEvent::TrustResolved { .. })));
    }
}
