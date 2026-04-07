use serde::{Deserialize, Serialize};

use crate::attachments::Attachment;
use crate::session::{ContentBlock, SessionError};

/// Rich message part enum supporting text, reasoning, tool calls, tool results,
/// and step tracking for structured AI assistant conversations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text {
        content: String,
    },
    Reasoning {
        content: String,
    },
    ToolCall {
        id: String,
        name: String,
        input: String,
    },
    ToolResult {
        tool_use_id: String,
        tool_name: String,
        output: String,
        is_error: bool,
    },
    StepStart {
        step_id: String,
        description: String,
    },
    StepFinish {
        step_id: String,
        summary: String,
        success: bool,
    },
    Attachment {
        attachment: Attachment,
    },
}

impl MessagePart {
    /// Serialize this part to a JSON string using `serde_json`.
    pub fn to_json_string(&self) -> Result<String, SessionError> {
        serde_json::to_string(self).map_err(|e| SessionError::Format(e.to_string()))
    }

    /// Deserialize a part from a JSON string using `serde_json`.
    pub fn from_json_string(s: &str) -> Result<Self, SessionError> {
        serde_json::from_str(s).map_err(|e| SessionError::Format(e.to_string()))
    }
}

// ─── Conversion: ContentBlock → MessagePart ─────────────────────────────────

impl From<ContentBlock> for MessagePart {
    fn from(block: ContentBlock) -> Self {
        match block {
            ContentBlock::Text { text } => MessagePart::Text { content: text },
            ContentBlock::ToolUse { id, name, input } => MessagePart::ToolCall { id, name, input },
            ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => MessagePart::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            },
        }
    }
}

// ─── Conversion: MessagePart → ContentBlock (fallible) ──────────────────────

impl TryFrom<MessagePart> for ContentBlock {
    type Error = SessionError;

    fn try_from(part: MessagePart) -> Result<Self, Self::Error> {
        match part {
            MessagePart::Text { content } => Ok(ContentBlock::Text { text: content }),
            MessagePart::ToolCall { id, name, input } => {
                Ok(ContentBlock::ToolUse { id, name, input })
            }
            MessagePart::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            } => Ok(ContentBlock::ToolResult {
                tool_use_id,
                tool_name,
                output,
                is_error,
            }),
            MessagePart::Reasoning { content } => Ok(ContentBlock::Text {
                text: format!("[reasoning] {content}"),
            }),
            MessagePart::StepStart {
                step_id,
                description,
            } => Ok(ContentBlock::Text {
                text: format!("[step_start] {step_id}: {description}"),
            }),
            MessagePart::StepFinish {
                step_id,
                summary,
                success,
            } => Ok(ContentBlock::Text {
                text: format!("[step_finish] {step_id}: {summary} (success={success})"),
            }),
            MessagePart::Attachment { attachment } => Ok(ContentBlock::Text {
                text: format!("[attachment: {}]", attachment.name),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MessagePart;
    use crate::attachments::Attachment;
    use crate::session::ContentBlock;

    // ─── Serialization roundtrip tests ──────────────────────────────────────

    #[test]
    fn text_serialization_roundtrip() {
        let part = MessagePart::Text {
            content: "Hello world".to_string(),
        };
        let json = part.to_json_string().expect("serialize text");
        let restored = MessagePart::from_json_string(&json).expect("deserialize text");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("\"content\":\"Hello world\""));
    }

    #[test]
    fn reasoning_serialization_roundtrip() {
        let part = MessagePart::Reasoning {
            content: "Let me think about this".to_string(),
        };
        let json = part.to_json_string().expect("serialize reasoning");
        let restored = MessagePart::from_json_string(&json).expect("deserialize reasoning");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"reasoning\""));
    }

    #[test]
    fn tool_call_serialization_roundtrip() {
        let part = MessagePart::ToolCall {
            id: "tc-1".to_string(),
            name: "bash".to_string(),
            input: "echo hi".to_string(),
        };
        let json = part.to_json_string().expect("serialize tool_call");
        let restored = MessagePart::from_json_string(&json).expect("deserialize tool_call");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"id\":\"tc-1\""));
        assert!(json.contains("\"name\":\"bash\""));
    }

    #[test]
    fn tool_result_serialization_roundtrip() {
        let part = MessagePart::ToolResult {
            tool_use_id: "tc-1".to_string(),
            tool_name: "bash".to_string(),
            output: "hi\n".to_string(),
            is_error: false,
        };
        let json = part.to_json_string().expect("serialize tool_result");
        let restored = MessagePart::from_json_string(&json).expect("deserialize tool_result");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"tool_result\""));
        assert!(json.contains("\"is_error\":false"));
    }

    #[test]
    fn step_start_serialization_roundtrip() {
        let part = MessagePart::StepStart {
            step_id: "s-1".to_string(),
            description: "Read source files".to_string(),
        };
        let json = part.to_json_string().expect("serialize step_start");
        let restored = MessagePart::from_json_string(&json).expect("deserialize step_start");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"step_start\""));
    }

    #[test]
    fn step_finish_serialization_roundtrip() {
        let part = MessagePart::StepFinish {
            step_id: "s-1".to_string(),
            summary: "Done".to_string(),
            success: true,
        };
        let json = part.to_json_string().expect("serialize step_finish");
        let restored = MessagePart::from_json_string(&json).expect("deserialize step_finish");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"step_finish\""));
        assert!(json.contains("\"success\":true"));
    }

    // ─── From<ContentBlock> conversion tests ────────────────────────────────

    #[test]
    fn content_block_text_to_message_part() {
        let block = ContentBlock::Text {
            text: "Hello".to_string(),
        };
        let part: MessagePart = block.clone().into();
        assert_eq!(
            part,
            MessagePart::Text {
                content: "Hello".to_string()
            }
        );
    }

    #[test]
    fn content_block_tool_use_to_message_part() {
        let block = ContentBlock::ToolUse {
            id: "t-1".to_string(),
            name: "read".to_string(),
            input: "main.rs".to_string(),
        };
        let part: MessagePart = block.clone().into();
        assert_eq!(
            part,
            MessagePart::ToolCall {
                id: "t-1".to_string(),
                name: "read".to_string(),
                input: "main.rs".to_string(),
            }
        );
    }

    #[test]
    fn content_block_tool_result_to_message_part() {
        let block = ContentBlock::ToolResult {
            tool_use_id: "t-1".to_string(),
            tool_name: "read".to_string(),
            output: "content".to_string(),
            is_error: false,
        };
        let part: MessagePart = block.clone().into();
        assert_eq!(
            part,
            MessagePart::ToolResult {
                tool_use_id: "t-1".to_string(),
                tool_name: "read".to_string(),
                output: "content".to_string(),
                is_error: false,
            }
        );
    }

    // ─── TryFrom<MessagePart> for ContentBlock tests ────────────────────────

    #[test]
    fn message_part_text_to_content_block() {
        let part = MessagePart::Text {
            content: "Hello".to_string(),
        };
        let block: ContentBlock = part.try_into().expect("convert text");
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "Hello".to_string()
            }
        );
    }

    #[test]
    fn message_part_tool_call_to_content_block() {
        let part = MessagePart::ToolCall {
            id: "t-1".to_string(),
            name: "bash".to_string(),
            input: "ls".to_string(),
        };
        let block: ContentBlock = part.try_into().expect("convert tool_call");
        assert_eq!(
            block,
            ContentBlock::ToolUse {
                id: "t-1".to_string(),
                name: "bash".to_string(),
                input: "ls".to_string(),
            }
        );
    }

    #[test]
    fn message_part_tool_result_to_content_block() {
        let part = MessagePart::ToolResult {
            tool_use_id: "t-1".to_string(),
            tool_name: "bash".to_string(),
            output: "ok".to_string(),
            is_error: false,
        };
        let block: ContentBlock = part.try_into().expect("convert tool_result");
        assert_eq!(
            block,
            ContentBlock::ToolResult {
                tool_use_id: "t-1".to_string(),
                tool_name: "bash".to_string(),
                output: "ok".to_string(),
                is_error: false,
            }
        );
    }

    #[test]
    fn message_part_reasoning_to_content_block_prefixed() {
        let part = MessagePart::Reasoning {
            content: "I think so".to_string(),
        };
        let block: ContentBlock = part.try_into().expect("convert reasoning");
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "[reasoning] I think so".to_string()
            }
        );
    }

    #[test]
    fn message_part_step_start_to_content_block_prefixed() {
        let part = MessagePart::StepStart {
            step_id: "s-1".to_string(),
            description: "Analyze".to_string(),
        };
        let block: ContentBlock = part.try_into().expect("convert step_start");
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "[step_start] s-1: Analyze".to_string()
            }
        );
    }

    #[test]
    fn message_part_step_finish_to_content_block_prefixed() {
        let part = MessagePart::StepFinish {
            step_id: "s-1".to_string(),
            summary: "Done".to_string(),
            success: true,
        };
        let block: ContentBlock = part.try_into().expect("convert step_finish");
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "[step_finish] s-1: Done (success=true)".to_string()
            }
        );
    }

    // ─── Roundtrip: ContentBlock → MessagePart → ContentBlock ───────────────

    #[test]
    fn content_block_roundtrip_via_message_part() {
        let original = ContentBlock::ToolUse {
            id: "x".to_string(),
            name: "edit".to_string(),
            input: "{}".to_string(),
        };
        let part: MessagePart = original.clone().into();
        let restored: ContentBlock = part.try_into().expect("roundtrip");
        assert_eq!(original, restored);
    }

    #[test]
    fn message_part_attachment_serialization_roundtrip() {
        let att = Attachment::from_base64("image/png", "screenshot.png", "iVBORw0KGgo=");
        let part = MessagePart::Attachment {
            attachment: att.clone(),
        };
        let json = part.to_json_string().expect("serialize attachment part");
        let restored = MessagePart::from_json_string(&json).expect("deserialize attachment part");
        assert_eq!(part, restored);
        assert!(json.contains("\"type\":\"attachment\""));
        assert!(json.contains("\"mime_type\":\"image/png\""));
        assert!(json.contains("\"name\":\"screenshot.png\""));
    }

    #[test]
    fn message_part_attachment_to_content_block_fallback() {
        let att = Attachment::from_base64("image/png", "diagram.png", "data");
        let part = MessagePart::Attachment { attachment: att };
        let block: ContentBlock = part.try_into().expect("convert attachment");
        assert_eq!(
            block,
            ContentBlock::Text {
                text: "[attachment: diagram.png]".to_string()
            }
        );
    }
}
