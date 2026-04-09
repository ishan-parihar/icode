use serde_json::{json, Value};

/// JSON schema for the `background_output` tool.
#[must_use]
pub fn background_output_tool_spec() -> Value {
    json!({
        "name": "background_output",
        "description": "Get output from background task. Use full_session=true to fetch session messages with filters.",
        "parameters": {
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID to get output from (e.g., bg_1)"
                },
                "full_session": {
                    "type": "boolean",
                    "default": false,
                    "description": "Return full session messages with filters (default: false)"
                },
                "include_thinking": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include thinking/reasoning parts in full_session output"
                },
                "include_tool_results": {
                    "type": "boolean",
                    "default": false,
                    "description": "Include tool results in full_session output"
                },
                "timeout": {
                    "type": "integer",
                    "default": 60000,
                    "description": "Max wait time in ms (default: 60000, max: 600000)"
                },
                "block": {
                    "type": "boolean",
                    "default": false,
                    "description": "Wait for completion (default: false)"
                },
                "message_limit": {
                    "type": "integer",
                    "description": "Max messages to return (capped at 100)"
                },
                "since_message_id": {
                    "type": "string",
                    "description": "Return messages after this message ID (exclusive)"
                }
            },
            "required": ["task_id"]
        }
    })
}

/// JSON schema for the `background_cancel` tool.
#[must_use]
pub fn background_cancel_tool_spec() -> Value {
    json!({
        "name": "background_cancel",
        "description": "Cancel running background task(s). Use all=true to cancel ALL before final answer.",
        "parameters": {
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Task ID to cancel (required if all=false)"
                },
                "all": {
                    "type": "boolean",
                    "default": false,
                    "description": "Cancel all running background tasks (default: false)"
                }
            },
            "required": ["all"]
        }
    })
}

/// All background tool specs.
#[must_use]
pub fn background_tool_specs() -> Vec<Value> {
    vec![background_output_tool_spec(), background_cancel_tool_spec()]
}
