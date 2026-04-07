use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ToolSearchInput {
    pub query: String,
    pub max_results: Option<usize>,
}
