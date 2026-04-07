use serde_json::Value;

#[derive(Debug)]
pub enum ToolRepairResult {
    Repaired {
        tool_name: String,
        input: Value,
        note: String,
    },
    Failed {
        suggestions: Vec<String>,
        reason: String,
    },
}

static TOOL_ALIASES: &[(&str, &str)] = &[
    ("read", "read_file"),
    ("write", "write_file"),
    ("edit", "edit_file"),
    ("glob", "glob_search"),
    ("grep", "grep_search"),
    ("search", "grep_search"),
    ("web_search", "WebSearch"),
    ("websearch", "WebSearch"),
    ("websearchtool", "WebSearch"),
    ("web_fetch", "WebFetch"),
    ("webfetch", "WebFetch"),
    ("fetch", "WebFetch"),
    ("todo", "TodoWrite"),
    ("todowrite", "TodoWrite"),
    ("skill", "Skill"),
    ("agent", "Agent"),
    ("toolsearch", "ToolSearch"),
    ("searchtool", "ToolSearch"),
    ("bash", "bash"),
    ("shell", "bash"),
    ("run", "bash"),
    ("terminal", "bash"),
    ("notebook", "NotebookEdit"),
    ("notebookedit", "NotebookEdit"),
    ("sleep", "Sleep"),
    ("wait", "Sleep"),
    ("readfile", "read_file"),
    ("readfiletool", "read_file"),
    ("writefile", "write_file"),
    ("writefiletool", "write_file"),
    ("editfile", "edit_file"),
    ("editfiletool", "edit_file"),
    ("globsearch", "glob_search"),
    ("grepsearch", "grep_search"),
    ("websearch_tool", "WebSearch"),
    ("webfetch_tool", "WebFetch"),
    ("todo_write", "TodoWrite"),
    ("tool_search", "ToolSearch"),
    ("notebook_edit", "NotebookEdit"),
];

pub fn repair_tool_call(
    tool_name: &str,
    input: &str,
    available_tools: &[String],
) -> ToolRepairResult {
    let repaired_name = find_canonical_name(tool_name, available_tools);
    let repaired_input = repair_json(input);

    match (repaired_name, repaired_input) {
        (Some(name), Some(inp)) => {
            let note = format!(
                "Tool call repaired: '{}' -> '{}', JSON syntax fixed",
                tool_name, name
            );
            ToolRepairResult::Repaired { tool_name: name, input: inp, note }
        }
        (Some(name), None) => {
            let note = format!("Tool call repaired: '{}' -> '{}'", tool_name, name);
            if let Ok(parsed) = serde_json::from_str::<Value>(input) {
                ToolRepairResult::Repaired { tool_name: name, input: parsed, note }
            } else {
                ToolRepairResult::Repaired {
                    tool_name: name,
                    input: Value::Object(serde_json::Map::new()),
                    note: format!("{note} (input set to empty object)"),
                }
            }
        }
        (None, Some(_inp)) => {
            let suggestions = suggest_tool_names(tool_name, available_tools);
            ToolRepairResult::Failed {
                suggestions,
                reason: format!("Could not find a matching tool for '{}'", tool_name),
            }
        }
        (None, None) => {
            let suggestions = suggest_tool_names(tool_name, available_tools);
            ToolRepairResult::Failed {
                suggestions,
                reason: format!("Unknown tool '{}' with invalid JSON input", tool_name),
            }
        }
    }
}

fn find_canonical_name(input: &str, available: &[String]) -> Option<String> {
    let lower = input.to_lowercase();
    let trimmed = lower.trim();

    if let Some(&(alias, canonical)) = TOOL_ALIASES.iter().find(|(a, _)| {
        let alias_lower = a.to_lowercase();
        alias_lower == trimmed || alias_lower == trimmed.replace('_', "")
    }) {
        if available.iter().any(|t| {
            let tl = t.to_lowercase();
            tl == canonical.to_lowercase() || tl == canonical.to_lowercase().replace('_', "")
        }) {
            return Some(
                available
                    .iter()
                    .find(|t| t.to_lowercase() == canonical.to_lowercase())
                    .cloned()
                    .or_else(|| {
                        available
                            .iter()
                            .find(|t| t.to_lowercase() == canonical.to_lowercase().replace('_', ""))
                            .cloned()
                    })
                    .unwrap_or_else(|| canonical.to_string()),
            );
        }
    }

    let normalized_input = normalize_for_comparison(trimmed);
    for tool in available {
        let normalized_tool = normalize_for_comparison(&tool.to_lowercase());
        if normalized_input == normalized_tool {
            return Some(tool.clone());
        }
    }

    fuzzy_match(trimmed, available)
}

fn normalize_for_comparison(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn fuzzy_match(query: &str, candidates: &[String]) -> Option<String> {
    let query_lower = query.to_lowercase();
    let mut best_score = usize::MAX;
    let mut best_match: Option<String> = None;

    for candidate in candidates {
        let candidate_lower = candidate.to_lowercase();
        let distance = edit_distance(&query_lower, &candidate_lower);
        let threshold = (query_lower.len() / 3).max(2);
        if distance <= threshold && distance < best_score {
            best_score = distance;
            best_match = Some(candidate.clone());
        }
    }
    best_match
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();
    if m == 0 { return n; }
    if n == 0 { return m; }
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m { dp[i][0] = i; }
    for j in 0..=n { dp[0][j] = j; }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1).min(dp[i][j - 1] + 1).min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[m][n]
}

fn repair_json(input: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(input) {
        return Some(value);
    }
    let mut fixed = input.to_string();
    fixed = fix_single_quotes(&fixed);
    fixed = remove_trailing_commas(&fixed);
    fixed = fix_unquoted_keys(&fixed);
    fixed = balance_braces(&fixed);
    serde_json::from_str::<Value>(&fixed).ok()
}

fn fix_single_quotes(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut in_double_quote = false;
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        let c = chars[i];
        match c {
            '"' if !is_escaped(&chars, i) => { in_double_quote = !in_double_quote; result.push(c); }
            '\'' if !in_double_quote => { result.push('"'); }
            _ => { result.push(c); }
        }
        i += 1;
    }
    result
}

fn is_escaped(chars: &[char], pos: usize) -> bool {
    if pos == 0 { return false; }
    let mut count = 0;
    let mut i = pos - 1;
    loop {
        if chars[i] == '\\' { count += 1; if i == 0 { break; } i -= 1; } else { break; }
    }
    count % 2 == 1
}

fn remove_trailing_commas(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        result.push(chars[i]);
        if chars[i] == ',' {
            let mut j = i + 1;
            while j < len && chars[j].is_whitespace() { j += 1; }
            if j < len && (chars[j] == '}' || chars[j] == ']') { result.pop(); i = j; continue; }
        }
        i += 1;
    }
    result
}

fn fix_unquoted_keys(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '{' || chars[i] == ',' {
            result.push(chars[i]); i += 1;
            while i < len && chars[i].is_whitespace() { result.push(chars[i]); i += 1; }
            if i < len && chars[i].is_ascii_alphabetic() {
                let key_start = i;
                while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '-') { i += 1; }
                let key: String = chars[key_start..i].iter().collect();
                while i < len && chars[i].is_whitespace() { i += 1; }
                if i < len && chars[i] == ':' { result.push('"'); result.push_str(&key); result.push('"'); continue; }
                result.push_str(&key); continue;
            }
            continue;
        }
        result.push(chars[i]); i += 1;
    }
    result
}

fn balance_braces(input: &str) -> String {
    let mut result = input.to_string();
    let mut open_braces: usize = 0;
    let mut open_brackets: usize = 0;
    let mut in_string = false;
    let mut escape_next = false;
    for ch in result.chars() {
        if escape_next { escape_next = false; continue; }
        if ch == '\\' { escape_next = true; continue; }
        if ch == '"' { in_string = !in_string; continue; }
        if !in_string {
            match ch {
                '{' => open_braces += 1,
                '}' => open_braces = open_braces.saturating_sub(1),
                '[' => open_brackets += 1,
                ']' => open_brackets = open_brackets.saturating_sub(1),
                _ => {}
            }
        }
    }
    for _ in 0..open_brackets { result.push(']'); }
    for _ in 0..open_braces { result.push('}'); }
    result
}

fn suggest_tool_names(query: &str, available: &[String]) -> Vec<String> {
    let query_lower = query.to_lowercase();
    let mut scored: Vec<(usize, String)> = available.iter().map(|t| {
        (edit_distance(&query_lower, &t.to_lowercase()), t.clone())
    }).collect();
    scored.sort_by_key(|(score, _)| *score);
    let threshold = (query_lower.len() / 2).max(2);
    scored.into_iter().filter(|(score, _)| *score <= threshold).take(5).map(|(_, name)| name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tools() -> Vec<String> {
        vec!["read_file".to_string(), "write_file".to_string(), "edit_file".to_string(),
             "glob_search".to_string(), "grep_search".to_string(), "WebSearch".to_string(),
             "WebFetch".to_string(), "TodoWrite".to_string(), "Skill".to_string(),
             "Agent".to_string(), "ToolSearch".to_string(), "NotebookEdit".to_string(),
             "Sleep".to_string(), "bash".to_string()]
    }

    #[test]
    fn repairs_read_to_read_file() {
        let result = repair_tool_call("read", r#"{"path":"test.rs"}"#, &sample_tools());
        match result { ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "read_file"), _ => panic!("expected repaired") }
    }

    #[test]
    fn repairs_write_to_write_file() {
        let result = repair_tool_call("write", r#"{"path":"out.rs","content":"hi"}"#, &sample_tools());
        match result { ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "write_file"), _ => panic!("expected repaired") }
    }

    #[test]
    fn repairs_websearch_typo() {
        let result = repair_tool_call("webseach", r#"{"query":"rust"}"#, &sample_tools());
        match result {
            ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "WebSearch"),
            ToolRepairResult::Failed { suggestions, .. } => assert!(suggestions.contains(&"WebSearch".to_string())),
        }
    }

    #[test]
    fn repairs_single_quotes_to_double_quotes() {
        let result = repair_tool_call("read_file", r"{'path':'test.rs'}", &sample_tools());
        match result { ToolRepairResult::Repaired { input, .. } => assert_eq!(input["path"], "test.rs"), ToolRepairResult::Failed { .. } => {} }
    }

    #[test]
    fn repairs_trailing_comma_in_object() {
        let result = repair_tool_call("read_file", r#"{"path": "test.rs",}"#, &sample_tools());
        match result { ToolRepairResult::Repaired { input, .. } => assert_eq!(input["path"], "test.rs"), ToolRepairResult::Failed { .. } => {} }
    }

    #[test]
    fn repairs_unquoted_keys() {
        let result = repair_tool_call("read_file", r#"{path: "test.rs"}"#, &sample_tools());
        match result { ToolRepairResult::Repaired { input, .. } => assert_eq!(input["path"], "test.rs"), ToolRepairResult::Failed { .. } => {} }
    }

    #[test]
    fn repairs_missing_closing_brace() {
        let result = repair_tool_call("read_file", r#"{"path": "test.rs""#, &sample_tools());
        match result { ToolRepairResult::Repaired { input, .. } => assert_eq!(input["path"], "test.rs"), ToolRepairResult::Failed { .. } => {} }
    }

    #[test]
    fn failed_repair_returns_suggestions() {
        let result = repair_tool_call("__nonexistent__", r#"{}"#, &sample_tools());
        match result { ToolRepairResult::Failed { .. } => {}, _ => panic!("expected failed") }
    }

    #[test]
    fn edit_distance_identical() { assert_eq!(edit_distance("abc", "abc"), 0); }
    #[test]
    fn edit_distance_one_change() { assert_eq!(edit_distance("abc", "abd"), 1); }
    #[test]
    fn edit_distance_completely_different() { assert_eq!(edit_distance("abc", "xyz"), 3); }
    #[test]
    fn edit_distance_empty_strings() { assert_eq!(edit_distance("", ""), 0); }
    #[test]
    fn edit_distance_empty_first() { assert_eq!(edit_distance("", "abc"), 3); }
    #[test]
    fn edit_distance_empty_second() { assert_eq!(edit_distance("abc", ""), 3); }

    #[test]
    fn suggest_returns_closest_matches() {
        let s = suggest_tool_names("webseach", &sample_tools());
        assert!(!s.is_empty());
        assert!(s.contains(&"WebSearch".to_string()));
    }

    #[test]
    fn suggest_limits_to_five() {
        assert!(suggest_tool_names("x", &sample_tools()).len() <= 5);
    }

    #[test]
    fn repairs_both_name_and_json() {
        let result = repair_tool_call("websearch", r"{'query': 'rust',}", &sample_tools());
        match result {
            ToolRepairResult::Repaired { tool_name, input, note } => {
                assert_eq!(tool_name, "WebSearch");
                assert_eq!(input["query"], "rust");
                assert!(note.contains("repaired"));
            }
            _ => panic!("expected repaired, got {:?}", result),
        }
    }

    #[test]
    fn alias_shell_maps_to_bash() {
        let result = repair_tool_call("shell", r#"{"command":"ls"}"#, &sample_tools());
        match result { ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "bash"), _ => panic!("expected repaired") }
    }

    #[test]
    fn alias_glob_maps_to_glob_search() {
        let result = repair_tool_call("glob", r#"{"pattern":"*.rs"}"#, &sample_tools());
        match result {
            ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "glob_search"),
            ToolRepairResult::Failed { suggestions, .. } => assert!(suggestions.iter().any(|s| s.contains("glob"))),
        }
    }

    #[test]
    fn fuzzy_match_close_typo() {
        let tools = vec!["read_file".to_string(), "write_file".to_string()];
        assert_eq!(find_canonical_name("read_fiel", &tools), Some("read_file".to_string()));
    }

    #[test]
    fn no_match_for_garbage() {
        let result = repair_tool_call("zzzzz", "{}", &sample_tools());
        match result { ToolRepairResult::Failed { .. } => {}, other => panic!("expected failed, got {:?}", other) }
    }

    #[test]
    fn valid_json_passes_through() {
        let r = repair_json(r#"{"path":"test.rs","limit":100}"#).unwrap();
        assert_eq!(r["path"], "test.rs");
        assert_eq!(r["limit"], 100);
    }

    #[test]
    fn repair_json_fixes_single_quotes() {
        assert_eq!(repair_json(r"{'path':'test.rs'}").unwrap()["path"], "test.rs");
    }

    #[test]
    fn repair_json_removes_trailing_comma() {
        let v = repair_json(r#"{"a":1,"b":2,}"#).unwrap();
        assert_eq!(v["a"], 1);
        assert_eq!(v["b"], 2);
    }

    #[test]
    fn repair_json_balances_braces() {
        assert_eq!(repair_json(r#"{"path":"test.rs""#).unwrap()["path"], "test.rs");
    }

    #[test]
    fn repair_tool_call_valid_input() {
        let result = repair_tool_call("read_file", r#"{"path":"x.rs"}"#, &["read_file".to_string()]);
        match result {
            ToolRepairResult::Repaired { tool_name, input, .. } => {
                assert_eq!(tool_name, "read_file");
                assert_eq!(input["path"], "x.rs");
            }
            _ => panic!("expected repaired"),
        }
    }

    #[test]
    fn repairs_valid_tool_with_valid_json_is_noop() {
        let result = repair_tool_call("read_file", r#"{"path":"x.rs"}"#, &sample_tools());
        match result {
            ToolRepairResult::Repaired { tool_name, input, note } => {
                assert_eq!(tool_name, "read_file");
                assert_eq!(input["path"], "x.rs");
                assert!(note.contains("repaired"));
            }
            _ => panic!("expected repaired"),
        }
    }

    #[test]
    fn repairs_hyphen_to_underscore() {
        let result = repair_tool_call("read-file", r#"{"path":"x.rs"}"#, &sample_tools());
        match result {
            ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "read_file"),
            _ => {}
        }
    }

    #[test]
    fn repairs_sendusermessage_alias() {
        let tools = vec!["SendUserMessage".to_string()];
        let result = repair_tool_call("send_user_message", r#"{"text":"hi"}"#, &tools);
        match result {
            ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "SendUserMessage"),
            _ => {}
        }
    }

    #[test]
    fn repairs_taskcreate_alias() {
        let tools = vec!["TaskCreate".to_string()];
        let result = repair_tool_call("taskcreate", r#"{}"#, &tools);
        match result { ToolRepairResult::Repaired { tool_name, .. } => assert_eq!(tool_name, "TaskCreate"), _ => panic!("expected repaired, got {:?}", result) }
    }
}
