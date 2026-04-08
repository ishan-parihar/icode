use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[must_use]
#[allow(clippy::match_same_arms)]
pub fn tool_risk_level(tool_name: &str) -> RiskLevel {
    match tool_name {
        "read_file" | "glob_search" | "grep_search" | "WebFetch" | "WebSearch" | "ToolSearch"
        | "Skill" | "Sleep" | "SendUserMessage" | "AskUserQuestion" | "TaskGet" | "TaskList"
        | "TaskOutput" | "WorkerGet" | "WorkerObserve" | "WorkerAwaitReady"
        | "ListMcpResources" | "ReadMcpResource" | "McpAuth" | "LSP" | "CronList"
        | "StructuredOutput" | "ls" => RiskLevel::Low,

        "write_file" | "edit_file" | "NotebookEdit" | "BatchEdit" | "ApplyPatch" | "Formatter"
        | "TodoWrite" | "Config" | "EnterPlanMode" | "ExitPlanMode" | "EnterWorktree"
        | "ExitWorktree" => RiskLevel::Medium,

        "bash" | "PowerShell" | "REPL" | "Agent" | "TaskCreate" | "RunTaskPacket" | "TaskStop"
        | "TaskUpdate" | "WorkerCreate" | "WorkerRestart" | "WorkerTerminate"
        | "WorkerResolveTrust" | "WorkerSendPrompt" | "TeamCreate" | "TeamDelete"
        | "CronCreate" | "CronDelete" | "MCP" | "RemoteTrigger" | "PtyBash"
        | "TestingPermission" => RiskLevel::High,

        _ => RiskLevel::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_low_risk_tools() {
        assert_eq!(tool_risk_level("read_file"), RiskLevel::Low);
        assert_eq!(tool_risk_level("glob_search"), RiskLevel::Low);
        assert_eq!(tool_risk_level("grep_search"), RiskLevel::Low);
        assert_eq!(tool_risk_level("WebFetch"), RiskLevel::Low);
        assert_eq!(tool_risk_level("WebSearch"), RiskLevel::Low);
        assert_eq!(tool_risk_level("Skill"), RiskLevel::Low);
        assert_eq!(tool_risk_level("Sleep"), RiskLevel::Low);
        assert_eq!(tool_risk_level("TaskGet"), RiskLevel::Low);
        assert_eq!(tool_risk_level("TaskList"), RiskLevel::Low);
        assert_eq!(tool_risk_level("ls"), RiskLevel::Low);
    }

    #[test]
    fn test_medium_risk_tools() {
        assert_eq!(tool_risk_level("write_file"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("edit_file"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("NotebookEdit"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("BatchEdit"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("ApplyPatch"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("Formatter"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("TodoWrite"), RiskLevel::Medium);
    }

    #[test]
    fn test_high_risk_tools() {
        assert_eq!(tool_risk_level("bash"), RiskLevel::High);
        assert_eq!(tool_risk_level("PowerShell"), RiskLevel::High);
        assert_eq!(tool_risk_level("REPL"), RiskLevel::High);
        assert_eq!(tool_risk_level("Agent"), RiskLevel::High);
        assert_eq!(tool_risk_level("TaskCreate"), RiskLevel::High);
        assert_eq!(tool_risk_level("PtyBash"), RiskLevel::High);
    }

    #[test]
    fn test_unknown_tool_defaults_to_medium() {
        assert_eq!(tool_risk_level("unknown_tool"), RiskLevel::Medium);
        assert_eq!(tool_risk_level("foo"), RiskLevel::Medium);
    }
}
