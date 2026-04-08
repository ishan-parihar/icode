use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A single field in an elicitation form.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct ElicitationField {
    pub field_id: String,
    pub label: String,
    pub field_type: String,
    pub description: Option<String>,
    pub options: Option<Vec<String>>,
    pub required: Option<bool>,
}

/// Input for the Elicitation tool — defines a form to present to the user.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ElicitationInput {
    pub title: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    pub fields: Vec<ElicitationField>,
}

/// Output returned by the Elicitation tool.
#[derive(Debug, Serialize)]
pub struct ElicitationOutput {
    pub status: String,
    pub title: String,
    pub responses: Option<BTreeMap<String, String>>,
    pub message: String,
}

/// Execute an elicitation request.
///
/// Validates the input schema and returns a structured response indicating
/// the elicitation form has been prepared. The actual user interaction
/// occurs at the TUI/REPL level.
pub fn execute_elicitation(input: ElicitationInput) -> Result<ElicitationOutput, String> {
    // Validate fields is not empty
    if input.fields.is_empty() {
        return Err("Elicitation requires at least one field".to_string());
    }

    // Validate unique field_ids
    let mut seen_ids = BTreeMap::new();
    for field in &input.fields {
        if seen_ids.contains_key(&field.field_id) {
            return Err(format!("Duplicate field_id: {}", field.field_id));
        }
        seen_ids.insert(&field.field_id, true);
    }

    // Validate each field_type and select-specific constraints
    for field in &input.fields {
        match field.field_type.as_str() {
            "text" | "select" | "number" => {}
            _ => {
                return Err(format!(
                    "Invalid field_type '{}'. Must be 'text', 'select', or 'number'",
                    field.field_type
                ));
            }
        }

        if field.field_type == "select" {
            match &field.options {
                Some(opts) if !opts.is_empty() => {}
                _ => {
                    return Err(format!(
                        "Select field '{}' requires options",
                        field.field_id
                    ));
                }
            }
        }
    }

    // Build a descriptive message
    let field_details: Vec<String> = input
        .fields
        .iter()
        .map(|f| {
            let type_info = match f.field_type.as_str() {
                "select" => {
                    let opts = f.options.as_ref().map(|o| o.join(", ")).unwrap_or_default();
                    format!("select [{opts}]")
                }
                other => other.to_string(),
            };
            format!("  - {}: {} ({})", f.label, type_info, f.field_id)
        })
        .collect();

    let message = format!(
        "Requesting user input: {}\n{}",
        input.title,
        field_details.join("\n")
    );

    Ok(ElicitationOutput {
        status: "pending".to_string(),
        title: input.title,
        responses: None,
        message,
    })
}

/// Return the tool spec as a JSON Value for registration.
pub fn elicitation_tool_spec() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(ElicitationInput)).unwrap()
}
