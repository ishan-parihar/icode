use orchestration::{
    builtin_agents, builtin_categories, AgentRegistry, CategoryResolver, PromptBuilder,
    TaskExecutor, TaskInput, TaskOutput, TaskStatus,
};

// === Deserialization ===

#[test]
fn task_input_deserializes_with_category() {
    let json = serde_json::json!({
        "category": "quick",
        "prompt": "fix typo in main.rs"
    });
    let input: TaskInput = serde_json::from_value(json).unwrap();
    assert_eq!(input.category.as_deref(), Some("quick"));
    assert_eq!(input.prompt, "fix typo in main.rs");
    assert!(input.subagent_type.is_none());
}

#[test]
fn task_input_deserializes_with_subagent() {
    let json = serde_json::json!({
        "subagent_type": "oracle",
        "prompt": "analyze architecture"
    });
    let input: TaskInput = serde_json::from_value(json).unwrap();
    assert_eq!(input.subagent_type.as_deref(), Some("oracle"));
    assert!(input.category.is_none());
}

#[test]
fn task_input_defaults() {
    let json = serde_json::json!({
        "category": "quick",
        "prompt": "test"
    });
    let input: TaskInput = serde_json::from_value(json).unwrap();
    assert!(!input.run_in_background);
    assert!(input.load_skills.is_empty());
}

// === Validation ===

#[test]
fn validate_requires_category_or_subagent() {
    let input = TaskInput {
        category: None,
        subagent_type: None,
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    assert!(TaskExecutor::validate(&input).is_err());
}

#[test]
fn validate_rejects_both() {
    let input = TaskInput {
        category: Some("quick".into()),
        subagent_type: Some("oracle".into()),
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    assert!(TaskExecutor::validate(&input).is_err());
}

#[test]
fn validate_accepts_category_only() {
    let input = TaskInput {
        category: Some("quick".into()),
        subagent_type: None,
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    assert!(TaskExecutor::validate(&input).is_ok());
}

#[test]
fn validate_accepts_subagent_only() {
    let input = TaskInput {
        category: None,
        subagent_type: Some("oracle".into()),
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    assert!(TaskExecutor::validate(&input).is_ok());
}

// === Agent Resolution ===

#[test]
fn resolve_agent_from_category() {
    let mut registry = AgentRegistry::new();
    for agent in builtin_agents() {
        registry.register(agent);
    }
    let resolver = CategoryResolver::with_overrides(builtin_categories());
    let executor = TaskExecutor::new(registry, resolver);

    let input = TaskInput {
        category: Some("quick".into()),
        subagent_type: None,
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    let agent = executor.resolve_agent(&input).unwrap();
    assert_eq!(agent.name, "quick");
    assert_eq!(agent.model, "openai/gpt-5.4-mini");
}

#[test]
fn resolve_agent_from_category_with_skills() {
    let mut registry = AgentRegistry::new();
    for agent in builtin_agents() {
        registry.register(agent);
    }
    let resolver = CategoryResolver::with_overrides(builtin_categories());
    let executor = TaskExecutor::new(registry, resolver);

    let input = TaskInput {
        category: Some("visual-engineering".into()),
        subagent_type: None,
        prompt: "test".into(),
        load_skills: vec!["frontend-design".into()],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    let agent = executor.resolve_agent(&input).unwrap();
    assert_eq!(agent.name, "visual-engineering");
}

#[test]
fn resolve_agent_from_subagent() {
    let mut registry = AgentRegistry::new();
    for agent in builtin_agents() {
        registry.register(agent);
    }
    let resolver = CategoryResolver::new();
    let executor = TaskExecutor::new(registry, resolver);

    let input = TaskInput {
        category: None,
        subagent_type: Some("oracle".into()),
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    let agent = executor.resolve_agent(&input).unwrap();
    assert_eq!(agent.name, "oracle");
}

#[test]
fn resolve_unknown_category_returns_error() {
    let registry = AgentRegistry::new();
    let resolver = CategoryResolver::new();
    let executor = TaskExecutor::new(registry, resolver);

    let input = TaskInput {
        category: Some("nonexistent".into()),
        subagent_type: None,
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    assert!(executor.resolve_agent(&input).is_err());
}

#[test]
fn resolve_unknown_subagent_returns_error() {
    let registry = AgentRegistry::new();
    let resolver = CategoryResolver::new();
    let executor = TaskExecutor::new(registry, resolver);

    let input = TaskInput {
        category: None,
        subagent_type: Some("unknown-agent".into()),
        prompt: "test".into(),
        load_skills: vec![],
        run_in_background: false,
        description: None,
        session_id: None,
        blocked_by: None,
        blocks: None,
    };
    assert!(executor.resolve_agent(&input).is_err());
}

// === Prompt Builder ===

#[test]
fn prompt_builder_injects_skills() {
    let result = PromptBuilder::build(
        "Fix the bug in main.rs",
        &["frontend-design".into()],
        &["Design beautiful UIs".into()],
    );
    assert!(result.contains("[SKILL CONTEXT]"));
    assert!(result.contains("## Skill: frontend-design"));
    assert!(result.contains("Design beautiful UIs"));
    assert!(result.contains("---"));
    assert!(result.contains("Fix the bug in main.rs"));
}

#[test]
fn prompt_builder_empty_skills_returns_base() {
    let base = "Just the base prompt";
    let result = PromptBuilder::build(base, &[], &[]);
    assert_eq!(result, base);
}

#[test]
fn prompt_builder_multiple_skills() {
    let result = PromptBuilder::build(
        "Build the feature",
        &["skill-a".into(), "skill-b".into()],
        &["Prompt A".into(), "Prompt B".into()],
    );
    assert!(result.contains("## Skill: skill-a"));
    assert!(result.contains("## Skill: skill-b"));
    assert!(result.contains("Prompt A"));
    assert!(result.contains("Prompt B"));
}

// === Serialization ===

#[test]
fn task_output_serializes() {
    let output = TaskOutput {
        task_id: "test-123".into(),
        session_id: "sess-456".into(),
        status: TaskStatus::Spawned,
        result: None,
    };
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("test-123"));
    assert!(json.contains("sess-456"));
    assert!(json.contains("Spawned"));
}

#[test]
fn task_status_completed_serializes() {
    let output = TaskOutput {
        task_id: "t1".into(),
        session_id: "s1".into(),
        status: TaskStatus::Completed("done".into()),
        result: Some("result".into()),
    };
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("Completed"));
    assert!(json.contains("done"));
    assert!(json.contains("result"));
}

#[test]
fn task_status_failed_serializes() {
    let output = TaskOutput {
        task_id: "t2".into(),
        session_id: "s2".into(),
        status: TaskStatus::Failed("error details".into()),
        result: None,
    };
    let json = serde_json::to_string(&output).unwrap();
    assert!(json.contains("Failed"));
    assert!(json.contains("error details"));
}
