use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WebFetchInput {
    pub url: String,
    pub prompt: String,
}
