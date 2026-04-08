use config::{
    load_jsonc_value, parse_jsonc, AgentConfig, BackgroundTaskConfig, Config, ConfigLoader,
    ConfigSource, HookConfig, RalphLoopConfig, SisyphusConfig,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir() -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("config-test-{nanos}"))
}

#[test]
fn parse_jsonc_with_single_line_comments() {
    let input = r#"{
        // model preference
        "model": "sonnet"
    }"#;
    let result = parse_jsonc(input).expect("should parse");
    assert_eq!(result["model"], "sonnet");
}

#[test]
fn parse_jsonc_with_block_comments() {
    let input = r#"{
        /* agent settings */
        "temperature": 0.7
    }"#;
    let result = parse_jsonc(input).expect("should parse");
    assert_eq!(result["temperature"], 0.7);
}

#[test]
fn parse_jsonc_with_trailing_commas() {
    let input = r#"{
        "a": 1,
        "b": 2,
        "c": [1, 2, 3,],
    }"#;
    let result = parse_jsonc(input).expect("should parse");
    assert_eq!(result["a"], 1);
    assert_eq!(result["b"], 2);
    assert_eq!(result["c"].as_array().unwrap().len(), 3);
}

#[test]
fn parse_jsonc_with_single_quoted_strings() {
    let input = "{'model': 'opus', 'temperature': 0.9}";
    let result = parse_jsonc(input).expect("should parse");
    assert_eq!(result["model"], "opus");
    assert_eq!(result["temperature"], 0.9);
}

#[test]
fn parse_jsonc_rejects_invalid_syntax() {
    let input = "{model: }";
    assert!(parse_jsonc(input).is_err());
}

#[test]
fn load_jsonc_value_into_struct() {
    let input = r#"{
        "model": "sonnet",
        "temperature": 0.7
    }"#;
    let value: serde_json::Value = load_jsonc_value(input).expect("should parse");
    assert_eq!(value["model"], "sonnet");
}

#[test]
fn agent_config_roundtrip() {
    let config = AgentConfig {
        model: Some("claude-sonnet-4-6".to_string()),
        temperature: Some(0.7),
        fallback_models: vec!["gpt-4o".to_string()],
        permissions: Some("workspace-write".to_string()),
    };
    let json = serde_json::to_string(&config).expect("should serialize");
    let restored: AgentConfig = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(config, restored);
}

#[test]
fn hook_config_roundtrip() {
    let config = HookConfig {
        disabled_hooks: vec!["pre-tool-use".to_string()],
    };
    let json = serde_json::to_string(&config).expect("should serialize");
    let restored: HookConfig = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(config, restored);
}

#[test]
fn background_task_config_roundtrip() {
    let config = BackgroundTaskConfig {
        max_concurrent: Some(4),
    };
    let json = serde_json::to_string(&config).expect("should serialize");
    let restored: BackgroundTaskConfig = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(config, restored);
}

#[test]
fn ralph_loop_config_roundtrip() {
    let config = RalphLoopConfig {
        enabled: true,
        default_max_iterations: 5,
    };
    let json = serde_json::to_string(&config).expect("should serialize");
    let restored: RalphLoopConfig = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(config, restored);
}

#[test]
fn sisyphus_config_roundtrip() {
    let config = SisyphusConfig {
        disabled: false,
        planner_enabled: true,
        replace_plan: true,
    };
    let json = serde_json::to_string(&config).expect("should serialize");
    let restored: SisyphusConfig = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(config, restored);
}

#[test]
fn config_defaults_serialize_correctly() {
    let config = Config::default();
    let json = serde_json::to_value(&config).expect("should serialize");
    assert!(json.is_object());
    let restored: Config = serde_json::from_value(json).expect("should deserialize");
    assert_eq!(config, restored);
}

#[test]
fn config_with_orchestration_fields() {
    let input = r#"{
        "model": "sonnet",
        "agents": {
            "explore": {
                "model": "haiku",
                "temperature": 0.3,
                "fallback_models": [],
                "permissions": "read-only"
            }
        },
        "hooks": {
            "disabled_hooks": ["pre-tool-use"]
        },
        "background_tasks": {
            "max_concurrent": 8
        },
        "ralph_loop": {
            "enabled": true,
            "default_max_iterations": 20
        },
        "sisyphus": {
            "disabled": false,
            "planner_enabled": true,
            "replace_plan": false
        }
    }"#;
    let config: Config = load_jsonc_value(input).expect("should parse");
    assert_eq!(config.model, Some("sonnet".to_string()));

    let agents = config.agents.expect("agents should exist");
    let explore = agents.get("explore").expect("explore agent should exist");
    assert_eq!(explore.model, Some("haiku".to_string()));
    assert_eq!(explore.temperature, Some(0.3));
    assert_eq!(explore.permissions, Some("read-only".to_string()));

    let hooks = config.hooks.expect("hooks should exist");
    assert_eq!(hooks.disabled_hooks, vec!["pre-tool-use".to_string()]);

    let bg = config
        .background_tasks
        .expect("background_tasks should exist");
    assert_eq!(bg.max_concurrent, Some(8));

    let ralph = config.ralph_loop.expect("ralph_loop should exist");
    assert!(ralph.enabled);
    assert_eq!(ralph.default_max_iterations, 20);

    let sisyphus = config.sisyphus.expect("sisyphus should exist");
    assert!(!sisyphus.disabled);
    assert!(sisyphus.planner_enabled);
}

#[test]
fn loader_uses_jsonc_for_config_with_comments() {
    let root = temp_dir();
    let cwd = root.join("project");
    let home = root.join("home").join(".icode");
    fs::create_dir_all(cwd.join(".icode")).expect("create project config dir");
    fs::create_dir_all(&home).expect("create home config dir");

    fs::write(
        cwd.join(".icode").join("settings.json"),
        r#"{
            // project model
            "model": "sonnet",
            "agents": {
                "explorer": {
                    "model": "haiku",
                    "fallback_models": [],
                },
            },
        }"#,
    )
    .expect("write config with comments");

    let cfg_loader = ConfigLoader::new(&cwd, &home);
    let loaded = cfg_loader.load().expect("should load");

    assert_eq!(loaded.typed().model, Some("sonnet".to_string()));
    let agents = loaded.typed().agents.as_ref().expect("agents should exist");
    assert_eq!(
        agents.get("explorer").unwrap().model,
        Some("haiku".to_string())
    );

    if root.exists() {
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }
}

#[test]
fn loader_backward_compat_with_plain_json() {
    let root = temp_dir();
    let cwd = root.join("project");
    let home = root.join("home").join(".icode");
    fs::create_dir_all(&home).expect("create home config dir");

    fs::write(
        home.join("settings.json"),
        r#"{"model":"opus","env":{"KEY":"value"}}"#,
    )
    .expect("write plain JSON config");

    let cfg_loader = ConfigLoader::new(&cwd, &home);
    let loaded = cfg_loader.load().expect("should load");

    assert_eq!(loaded.typed().model, Some("opus".to_string()));
    let env = loaded.typed().env.as_ref().expect("env should exist");
    assert_eq!(env.get("KEY"), Some(&"value".to_string()));

    if root.exists() {
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }
}

#[test]
fn config_discovery_order() {
    let root = temp_dir();
    let cwd = root.join("project");
    let home = root.join("home").join(".icode");
    fs::create_dir_all(&home).expect("create home config dir");

    let loader = ConfigLoader::new(&cwd, &home);
    let entries = loader.discover();

    assert_eq!(entries.len(), 5);
    assert_eq!(entries[0].source, ConfigSource::User);
    assert_eq!(entries[4].source, ConfigSource::Local);

    if root.exists() {
        fs::remove_dir_all(root).expect("cleanup temp dir");
    }
}
