use orchestration::background::tools::{
    background_cancel_tool_spec, background_output_tool_spec, background_tool_specs,
};

#[test]
fn background_output_spec_has_required_fields() {
    let spec = background_output_tool_spec();
    assert!(spec.get("name").is_some());
    assert!(spec.get("description").is_some());
    assert!(spec.get("parameters").is_some());
}

#[test]
fn background_output_requires_task_id() {
    let spec = background_output_tool_spec();
    let required = spec["parameters"]["required"].as_array().unwrap();
    assert!(required.iter().any(|v| v.as_str() == Some("task_id")));
}

#[test]
fn background_output_has_all_params() {
    let spec = background_output_tool_spec();
    let props = spec["parameters"]["properties"].as_object().unwrap();
    let expected = [
        "task_id",
        "full_session",
        "include_thinking",
        "include_tool_results",
        "timeout",
        "block",
        "message_limit",
        "since_message_id",
    ];
    for param in expected {
        assert!(props.contains_key(param), "missing param: {param}");
    }
}

#[test]
fn background_output_spec_name_correct() {
    let spec = background_output_tool_spec();
    assert_eq!(spec["name"].as_str().unwrap(), "background_output");
}

#[test]
fn background_cancel_spec_has_required_fields() {
    let spec = background_cancel_tool_spec();
    assert!(spec.get("name").is_some());
    assert!(spec.get("description").is_some());
    assert!(spec.get("parameters").is_some());
}

#[test]
fn background_cancel_has_params() {
    let spec = background_cancel_tool_spec();
    let props = spec["parameters"]["properties"].as_object().unwrap();
    assert!(props.contains_key("task_id"));
    assert!(props.contains_key("all"));
}

#[test]
fn background_cancel_all_defaults_to_false() {
    let spec = background_cancel_tool_spec();
    let all_param = &spec["parameters"]["properties"]["all"];
    assert_eq!(all_param["default"].as_bool(), Some(false));
}

#[test]
fn background_cancel_spec_name_correct() {
    let spec = background_cancel_tool_spec();
    assert_eq!(spec["name"].as_str().unwrap(), "background_cancel");
}

#[test]
fn background_tool_specs_returns_two() {
    let specs = background_tool_specs();
    assert_eq!(specs.len(), 2);
}
