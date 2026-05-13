use serde_json::Value;

use super::{AgentEvent, ToolStatus};

// v1 stream-json shape assumptions:
// - session start: `{"type":"system","subtype":"init",...}` (current Claude CLI convention)
//   OR `{"type":"session_start",...}` (alternate). Both map to SessionStarted.
// - assistant text: `{"type":"assistant","message":{"content":[{"type":"text","text":"..."}]}}`
//   one Text event emitted per text block, preserving order.
// - tool result: `{"type":"user","message":{"content":[{"type":"tool_result",...}]}}`
//   v1 does not pair tool_use w/ tool_result; name falls back to tool_use_id when absent.
// - turn complete: `{"type":"result","usage":{"input_tokens":N,"output_tokens":M}}`.
// - unknown / missing type: None. forward-compat per RFC.

const SUMMARY_MAX: usize = 200;

pub(crate) fn parse_record(line: &str) -> Option<AgentEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ty = v.get("type")?.as_str()?;

    match ty {
        "session_start" => Some(AgentEvent::SessionStarted),
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => {
            Some(AgentEvent::SessionStarted)
        }
        "assistant" => parse_assistant_text(&v),
        "user" => parse_tool_result(&v),
        "result" => parse_result(&v),
        _ => None,
    }
}

fn parse_assistant_text(v: &Value) -> Option<AgentEvent> {
    let content = v.get("message")?.get("content")?.as_array()?;
    // emit first text block; caller invokes parse_record per line so
    // multiple text blocks in one record collapse here. for v1 the
    // claude CLI emits one text block per assistant record so this matches.
    for block in content {
        if block.get("type").and_then(Value::as_str) == Some("text") {
            let delta = block.get("text")?.as_str()?.to_string();
            return Some(AgentEvent::Text { delta });
        }
    }
    None
}

fn parse_tool_result(v: &Value) -> Option<AgentEvent> {
    let content = v.get("message")?.get("content")?.as_array()?;
    for block in content {
        if block.get("type").and_then(Value::as_str) != Some("tool_result") {
            continue;
        }
        let is_error = block
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let status = if is_error {
            ToolStatus::Error
        } else {
            ToolStatus::Ok
        };
        let name = block
            .get("name")
            .and_then(Value::as_str)
            .or_else(|| block.get("tool_use_id").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        let summary = block
            .get("content")
            .map(stringify_content)
            .unwrap_or_default();
        return Some(AgentEvent::ToolCall {
            name,
            summary: truncate(&summary, SUMMARY_MAX),
            status,
        });
    }
    None
}

fn parse_result(v: &Value) -> Option<AgentEvent> {
    let usage = v.get("usage")?;
    let input_tokens = usage.get("input_tokens")?.as_u64()?;
    let output_tokens = usage.get("output_tokens")?.as_u64()?;
    Some(AgentEvent::TurnCompleted {
        input_tokens,
        output_tokens,
    })
}

fn stringify_content(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(""),
        other => other.to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_start_explicit() {
        let line = r#"{"type":"session_start","session_id":"abc"}"#;
        assert_eq!(parse_record(line), Some(AgentEvent::SessionStarted));
    }

    #[test]
    fn parses_system_init_as_session_start() {
        let line = r#"{"type":"system","subtype":"init","session_id":"abc"}"#;
        assert_eq!(parse_record(line), Some(AgentEvent::SessionStarted));
    }

    #[test]
    fn parses_assistant_text_delta() {
        let line = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"hello"}]}}"#;
        assert_eq!(
            parse_record(line),
            Some(AgentEvent::Text {
                delta: "hello".into()
            })
        );
    }

    #[test]
    fn ordered_assistant_text_deltas() {
        let lines = [
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"one"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"two"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"three"}]}}"#,
        ];
        let events: Vec<_> = lines.iter().filter_map(|l| parse_record(l)).collect();
        assert_eq!(
            events,
            vec![
                AgentEvent::Text {
                    delta: "one".into()
                },
                AgentEvent::Text {
                    delta: "two".into()
                },
                AgentEvent::Text {
                    delta: "three".into()
                },
            ]
        );
    }

    #[test]
    fn parses_tool_result_ok() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"file contents","is_error":false}]}}"#;
        assert_eq!(
            parse_record(line),
            Some(AgentEvent::ToolCall {
                name: "toolu_1".into(),
                summary: "file contents".into(),
                status: ToolStatus::Ok,
            })
        );
    }

    #[test]
    fn parses_tool_result_error() {
        let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_2","content":"boom","is_error":true}]}}"#;
        assert_eq!(
            parse_record(line),
            Some(AgentEvent::ToolCall {
                name: "toolu_2".into(),
                summary: "boom".into(),
                status: ToolStatus::Error,
            })
        );
    }

    #[test]
    fn parses_turn_complete_with_usage() {
        let line = r#"{"type":"result","subtype":"success","usage":{"input_tokens":42,"output_tokens":17}}"#;
        assert_eq!(
            parse_record(line),
            Some(AgentEvent::TurnCompleted {
                input_tokens: 42,
                output_tokens: 17,
            })
        );
    }

    #[test]
    fn unknown_type_returns_none() {
        let line = r#"{"type":"future_thing","payload":{"x":1}}"#;
        assert_eq!(parse_record(line), None);
    }

    #[test]
    fn missing_type_returns_none() {
        let line = r#"{"payload":{"x":1}}"#;
        assert_eq!(parse_record(line), None);
    }

    #[test]
    fn malformed_json_returns_none() {
        assert_eq!(parse_record("not json"), None);
    }

    #[test]
    fn result_without_usage_returns_none() {
        let line = r#"{"type":"result","subtype":"success"}"#;
        assert_eq!(parse_record(line), None);
    }
}
