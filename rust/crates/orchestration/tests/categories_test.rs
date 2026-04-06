use orchestration::{builtin_categories, CategoryConfig, CategoryResolver};

#[test]
fn builtin_categories_returns_8() {
    let cats = builtin_categories();
    assert_eq!(cats.len(), 8);
}

#[test]
fn each_category_has_required_fields() {
    let cats = builtin_categories();
    for cat in &cats {
        assert!(!cat.name.is_empty(), "category name is empty: {:?}", cat);
        assert!(
            !cat.description.is_empty(),
            "category description is empty: {:?}",
            cat
        );
        assert!(!cat.model.is_empty(), "category model is empty: {:?}", cat);
    }
}

#[test]
fn category_names_are_unique() {
    let cats = builtin_categories();
    let names: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
    let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
    assert_eq!(
        names.len(),
        unique.len(),
        "duplicate category names detected"
    );
}

#[test]
fn resolver_finds_all_categories() {
    let resolver = CategoryResolver::new();
    assert_eq!(resolver.available_categories().len(), 8);
}

#[test]
fn resolver_returns_agent_for_category() {
    let resolver = CategoryResolver::default();
    let agent = resolver.resolve("visual-engineering", &[], &[]);
    assert!(agent.is_some());
    let agent = agent.unwrap();
    assert_eq!(agent.name, "visual-engineering");
    assert!(!agent.prompt.is_empty());
}

#[test]
fn resolver_injects_skill_prompts() {
    let resolver = CategoryResolver::default();
    let agent = resolver.resolve(
        "visual-engineering",
        &["frontend-design".into()],
        &["Design beautiful UIs".into()],
    );
    assert!(agent.is_some());
    let agent = agent.unwrap();
    assert!(agent.prompt.contains("frontend-design"));
    assert!(agent.prompt.contains("Design beautiful UIs"));
}

#[test]
fn resolver_unknown_category_returns_none() {
    let resolver = CategoryResolver::default();
    let agent = resolver.resolve("nonexistent-category", &[], &[]);
    assert!(agent.is_none());
}

#[test]
fn resolver_with_overrides() {
    let override_config = CategoryConfig {
        name: "visual-engineering".into(),
        description: "Overridden frontend".into(),
        model: "openai/gpt-4".into(),
        ..Default::default()
    };
    let resolver = CategoryResolver::with_overrides(vec![override_config]);
    let agent = resolver.resolve("visual-engineering", &[], &[]).unwrap();
    assert_eq!(agent.model, "openai/gpt-4");
    assert_eq!(agent.name, "visual-engineering");
}

#[test]
fn visual_engineering_uses_gemini() {
    let resolver = CategoryResolver::default();
    let agent = resolver.resolve("visual-engineering", &[], &[]).unwrap();
    assert_eq!(agent.model, "google/gemini-3.1-pro");
}

#[test]
fn ultrabrain_uses_gpt_xhigh() {
    let resolver = CategoryResolver::default();
    let agent = resolver.resolve("ultrabrain", &[], &[]).unwrap();
    assert_eq!(agent.model, "openai/gpt-5.4");
    assert_eq!(agent.reasoning_effort, Some("xhigh".into()));
}

#[test]
fn quick_uses_mini_model() {
    let resolver = CategoryResolver::default();
    let agent = resolver.resolve("quick", &[], &[]).unwrap();
    assert_eq!(agent.model, "openai/gpt-5.4-mini");
}
