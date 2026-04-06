/// Parse JSONC (JSON with Comments) content into `serde_json::Value`.
///
/// Supports:
/// - `// single-line comments`
/// - `/* block comments */`
/// - Trailing commas
/// - Single-quoted strings
pub fn parse_jsonc(content: &str) -> Result<serde_json::Value, String> {
    json5::from_str(content).map_err(|e| e.to_string())
}

/// Load a typed value from JSONC string content.
pub fn load_jsonc_value<T: serde::de::DeserializeOwned>(content: &str) -> Result<T, String> {
    json5::from_str(content).map_err(|e| e.to_string())
}

/// Try parsing as JSONC first, fall back to strict JSON.
///
/// This maintains backward compatibility with existing plain JSON config files
/// while enabling JSONC features (comments, trailing commas) when present.
pub fn parse_config(content: &str) -> Result<serde_json::Value, String> {
    match parse_jsonc(content) {
        Ok(value) => Ok(value),
        Err(jsonc_err) => serde_json::from_str(content).map_err(|json_err| {
            format!("JSONC parse failed: {jsonc_err}; JSON parse failed: {json_err}")
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_jsonc_strips_single_line_comments() {
        let input = r#"{
            // this is a comment
            "key": "value"
        }"#;
        let result = parse_jsonc(input).expect("should parse");
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn parse_jsonc_strips_block_comments() {
        let input = r#"{
            /* block comment */
            "key": "value"
        }"#;
        let result = parse_jsonc(input).expect("should parse");
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn parse_jsonc_allows_trailing_commas() {
        let input = r#"{
            "a": 1,
            "b": 2,
        }"#;
        let result = parse_jsonc(input).expect("should parse");
        assert_eq!(result["a"], 1);
        assert_eq!(result["b"], 2);
    }

    #[test]
    fn parse_jsonc_allows_single_quoted_strings() {
        let input = "{'key': 'value'}";
        let result = parse_jsonc(input).expect("should parse");
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn parse_config_fallback_to_json() {
        let input = r#"{"key": "value"}"#;
        let result = parse_config(input).expect("should parse as JSON");
        assert_eq!(result["key"], "value");
    }
}
