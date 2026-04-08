//! Integration tests for cross-module wiring.
//!
//! These tests verify that adjacent modules in the runtime crate actually
//! connect correctly — catching wiring gaps that unit tests miss.

use std::time::Duration;

use runtime::green_contract::{GreenContract, GreenLevel};
use runtime::{
    apply_policy, BranchFreshness, DiffScope, LaneBlocker, LaneContext, PolicyAction,
    PolicyCondition, PolicyEngine, PolicyRule, ReviewStatus, StaleBranchAction, StaleBranchPolicy,
};

#[test]
fn stale_branch_detection_flows_into_policy_engine() {
    let stale_context = LaneContext::new(
        "stale-lane",
        0,
        Duration::from_secs(2 * 60 * 60),
        LaneBlocker::None,
        ReviewStatus::Pending,
        DiffScope::Full,
        false,
    );

    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "stale-merge-forward",
        PolicyCondition::StaleBranch,
        PolicyAction::MergeForward,
        10,
    )]);

    let actions = engine.evaluate(&stale_context);
    assert_eq!(actions, vec![PolicyAction::MergeForward]);
}

#[test]
fn fresh_branch_does_not_trigger_stale_policy() {
    let fresh_context = LaneContext::new(
        "fresh-lane",
        0,
        Duration::from_secs(30 * 60),
        LaneBlocker::None,
        ReviewStatus::Pending,
        DiffScope::Full,
        false,
    );

    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "stale-merge-forward",
        PolicyCondition::StaleBranch,
        PolicyAction::MergeForward,
        10,
    )]);

    let actions = engine.evaluate(&fresh_context);
    assert!(actions.is_empty());
}

#[test]
fn green_contract_satisfied_allows_merge() {
    let contract = GreenContract::new(GreenLevel::Workspace);
    let satisfied = contract.is_satisfied_by(GreenLevel::Workspace);
    assert!(satisfied);

    let exceeded = contract.is_satisfied_by(GreenLevel::MergeReady);
    assert!(exceeded);

    let insufficient = contract.is_satisfied_by(GreenLevel::Package);
    assert!(!insufficient);
}

#[test]
fn green_contract_unsatisfied_blocks_merge() {
    let context = LaneContext::new(
        "partial-green-lane",
        0,
        Duration::from_secs(0),
        LaneBlocker::None,
        ReviewStatus::Pending,
        DiffScope::Full,
        false,
    );

    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "workspace-green-required",
        PolicyCondition::GreenAt { level: 3 },
        PolicyAction::MergeToDev,
        10,
    )]);

    let actions = engine.evaluate(&context);
    assert!(actions.is_empty());
}

#[test]
fn reconciled_lane_matches_reconcile_condition() {
    let context = LaneContext::new(
        "completed-lane",
        3,
        Duration::from_secs(0),
        LaneBlocker::None,
        ReviewStatus::Approved,
        DiffScope::Full,
        true,
    );

    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "completed-closeout",
        PolicyCondition::LaneCompleted,
        PolicyAction::CloseoutLane,
        30,
    )]);

    let actions = engine.evaluate(&context);
    assert_eq!(actions, vec![PolicyAction::CloseoutLane]);
}

#[test]
fn stale_branch_apply_policy_produces_rebase_action() {
    let stale = BranchFreshness::Stale {
        commits_behind: 5,
        missing_fixes: vec!["fix-123".to_string()],
    };

    let action = apply_policy(&stale, StaleBranchPolicy::AutoRebase);
    assert_eq!(action, StaleBranchAction::Rebase);
}

#[test]
fn stale_branch_apply_policy_produces_merge_forward_action() {
    let stale = BranchFreshness::Stale {
        commits_behind: 3,
        missing_fixes: vec![],
    };

    let action = apply_policy(&stale, StaleBranchPolicy::AutoMergeForward);
    assert_eq!(action, StaleBranchAction::MergeForward);
}

#[test]
fn stale_branch_apply_policy_warn_only() {
    let stale = BranchFreshness::Stale {
        commits_behind: 2,
        missing_fixes: vec!["fix-456".to_string()],
    };

    let action = apply_policy(&stale, StaleBranchPolicy::WarnOnly);
    match action {
        StaleBranchAction::Warn { message } => {
            assert!(message.contains("2 commit(s) behind main"));
            assert!(message.contains("fix-456"));
        }
        _ => panic!("expected Warn action, got {action:?}"),
    }
}

#[test]
fn stale_branch_fresh_produces_noop() {
    let fresh = BranchFreshness::Fresh;
    let action = apply_policy(&fresh, StaleBranchPolicy::AutoRebase);
    assert_eq!(action, StaleBranchAction::Noop);
}

#[test]
fn end_to_end_stale_lane_gets_merge_forward_action() {
    let context = LaneContext::new(
        "lane-9411",
        3,
        Duration::from_secs(5 * 60 * 60),
        LaneBlocker::None,
        ReviewStatus::Approved,
        DiffScope::Scoped,
        false,
    );

    let engine = PolicyEngine::new(vec![
        PolicyRule::new(
            "auto-merge-forward-if-stale-and-approved",
            PolicyCondition::And(vec![
                PolicyCondition::StaleBranch,
                PolicyCondition::ReviewPassed,
            ]),
            PolicyAction::MergeForward,
            5,
        ),
        PolicyRule::new(
            "stale-warning",
            PolicyCondition::StaleBranch,
            PolicyAction::Notify {
                channel: "#build-status".to_string(),
            },
            10,
        ),
    ]);

    let actions = engine.evaluate(&context);
    assert_eq!(
        actions,
        vec![
            PolicyAction::MergeForward,
            PolicyAction::Notify {
                channel: "#build-status".to_string(),
            },
        ]
    );
}

#[test]
fn fresh_approved_lane_gets_merge_action() {
    let context = LaneContext::new(
        "fresh-approved-lane",
        3,
        Duration::from_secs(30 * 60),
        LaneBlocker::None,
        ReviewStatus::Approved,
        DiffScope::Scoped,
        false,
    );

    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "merge-if-green-approved-not-stale",
        PolicyCondition::And(vec![
            PolicyCondition::GreenAt { level: 3 },
            PolicyCondition::ReviewPassed,
        ]),
        PolicyAction::MergeToDev,
        5,
    )]);

    let actions = engine.evaluate(&context);
    assert_eq!(actions, vec![PolicyAction::MergeToDev]);
}

#[test]
fn worker_provider_failure_flows_through_recovery_to_policy() {
    use runtime::recovery_recipes::{
        attempt_recovery, FailureScenario, RecoveryContext, RecoveryResult,
    };
    use runtime::worker_boot::{WorkerRegistry, WorkerStatus};

    let registry = WorkerRegistry::new();
    let worker = registry.create("/tmp/repo-recovery-test", &[], true);

    registry
        .observe(&worker.worker_id, "Ready for your input\n>")
        .expect("ready observe should succeed");
    registry
        .send_prompt(&worker.worker_id, Some("Run analysis"))
        .expect("prompt send should succeed");

    let restarted = registry
        .restart(&worker.worker_id)
        .expect("restart should succeed");
    assert_eq!(restarted.status, WorkerStatus::Spawning);

    let mut ctx = RecoveryContext::new();
    let scenario = FailureScenario::McpHandshakeFailure;
    let result = attempt_recovery(&scenario, &mut ctx);

    assert!(
        matches!(result, RecoveryResult::Recovered { steps_taken: 1 }),
        "McpHandshakeFailure should recover via single RetryMcpHandshake step, got: {result:?}"
    );

    let recovery_success = matches!(result, RecoveryResult::Recovered { .. });

    let green_level = 3;
    let not_stale = Duration::from_secs(30 * 60);

    let post_recovery_context = LaneContext::new(
        "recovered-lane",
        green_level,
        not_stale,
        LaneBlocker::None,
        ReviewStatus::Approved,
        DiffScope::Scoped,
        false,
    );

    let policy_engine = PolicyEngine::new(vec![PolicyRule::new(
        "merge-after-successful-recovery",
        PolicyCondition::And(vec![
            PolicyCondition::GreenAt { level: 3 },
            PolicyCondition::ReviewPassed,
        ]),
        PolicyAction::MergeToDev,
        10,
    )]);

    assert!(
        recovery_success,
        "recovery must succeed for lane to proceed"
    );
    let actions = policy_engine.evaluate(&post_recovery_context);
    assert_eq!(
        actions,
        vec![PolicyAction::MergeToDev],
        "post-recovery green+approved lane should be merge-ready"
    );
}
