//! Context budgets of the chat worker loop (#1944): a character cap on each
//! tool reply with a truncation hint, a cap on attached viewport images, a
//! cap on tool rounds, the core tool set for local models, and rolling
//! history that keeps the latest `get_job` reply when it drops old turns.
//! Provenance: our own limits over our own chat transcript; no vendor material involved.

use camcraft_app::mcp::provider::ToolSet;
use serde_json::{Value, json};

use super::transport::{EndpointKind, TurnMessage};
use super::turn::ChatTurn;
use super::worker::ToolCardStatus;

/// The most characters of one tool reply that go back to the model.
pub(crate) const MAX_TOOL_REPLY_CHARS: usize = 12_000;
/// The most viewport images that one request carries (the newest win).
pub(crate) const MAX_IMAGES_PER_REQUEST: usize = 1;
/// The most model rounds of one turn before it stops without an answer.
pub(crate) const MAX_TOOL_ROUNDS: usize = 12;
/// The most earlier turns that one request carries (the newest win).
pub(crate) const MAX_PRIOR_TURNS: usize = 6;

/// A local model gets the core tool set; a cloud model the whole table.
#[must_use]
pub(super) fn tool_set_for(kind: EndpointKind) -> ToolSet {
    match kind {
        EndpointKind::Local => ToolSet::Core,
        EndpointKind::Cloud => ToolSet::Full,
    }
}

/// Characters one `char` takes in a JSON string (the provider sends the
/// tool result as serialized JSON text).
fn json_width(c: char) -> usize {
    match c {
        '"' | '\\' | '\n' | '\r' | '\t' => 2,
        c if (c as u32) < 0x20 => 6,
        _ => 1,
    }
}

/// The longest start of `text` whose JSON form is at most `max`
/// characters (never a split character), and whether it was cut.
fn head(text: &str, max: usize) -> (&str, bool) {
    let mut used = 0usize;
    for (at, c) in text.char_indices() {
        used = used.saturating_add(json_width(c));
        if used > max {
            return (&text[..at], true);
        }
    }
    (text, false)
}

/// `text` cut to `max` characters with a hint that says what to ask for.
#[must_use]
pub(super) fn cut_with_hint(text: &str, max: usize) -> String {
    let (kept, cut) = head(text, max);
    if !cut {
        return text.to_owned();
    }
    format!(
        "{kept}\n[truncated by the chat: {} of {} characters shown. Ask for less: \
         get_job {{\"summary\":true}}, list_operation_types {{\"kind\":...}}, \
         list_features {{\"compact\":true}}.]",
        kept.chars().count(),
        text.chars().count()
    )
}

/// Cap one MCP tool result before it goes back to the model. A result
/// over the cap becomes one text block (the structured copy is dropped)
/// that ends with the truncation hint; the error flag stays.
#[must_use]
pub(super) fn cap_tool_reply(result: Value, text: String) -> (Value, String) {
    if result.to_string().chars().count() <= MAX_TOOL_REPLY_CHARS {
        return (result, text);
    }
    let cut = cut_with_hint(&text, MAX_TOOL_REPLY_CHARS);
    let is_error = result.get("isError").and_then(Value::as_bool).unwrap_or(false);
    (json!({ "content": [{ "type": "text", "text": cut }], "isError": is_error }), cut)
}

/// Keep the newest `MAX_IMAGES_PER_REQUEST` screenshots; drop the older ones.
pub(super) fn cap_images(messages: &mut [TurnMessage]) {
    let mut seen = 0usize;
    for m in messages.iter_mut().rev() {
        if m.screenshot.is_some() {
            seen += 1;
            if seen > MAX_IMAGES_PER_REQUEST {
                m.screenshot = None;
            }
        }
    }
}

/// The earlier turns that a request carries, and - when older turns were
/// dropped - the latest successful `get_job` reply of all earlier turns,
/// cut to the reply cap, so the model keeps the state of the job.
#[must_use]
pub(super) fn prior_window(prior: &[ChatTurn]) -> (&[ChatTurn], Option<String>) {
    let skip = prior.len().saturating_sub(MAX_PRIOR_TURNS);
    let kept = prior.get(skip..).unwrap_or(&[]);
    if skip == 0 {
        return (kept, None);
    }
    let job = prior
        .iter()
        .rev()
        .flat_map(|t| t.tool_cards.iter().rev())
        .find(|c| c.name == "get_job" && c.status == ToolCardStatus::Succeeded && !c.summary.is_empty())
        .map(|c| {
            format!(
                "[Context: {skip} earlier turns were dropped. The latest get_job reply follows.]\n{}\n\n",
                cut_with_hint(&c.summary, MAX_TOOL_REPLY_CHARS)
            )
        });
    (kept, job)
}

#[cfg(test)]
mod tests {
    use super::super::transport::ScreenshotAttachment;
    use super::super::turn::build_turn_messages;
    use super::super::worker::ToolCard;
    use super::*;
    use camcraft_app::commands::History;

    fn shot() -> Option<ScreenshotAttachment> {
        Some(ScreenshotAttachment { mime_type: "image/png".into(), data_base64: "AAAA".into() })
    }

    fn turn(id: u64, with_job: Option<&str>) -> ChatTurn {
        let mut t = ChatTurn::running(id, format!("prompt {id}"), &History::default(), None, shot());
        t.assistant_text = format!("answer {id}");
        if let Some(text) = with_job {
            t.tool_cards.push(ToolCard {
                id: format!("c{id}"),
                name: "get_job".into(),
                arguments: json!({}),
                status: ToolCardStatus::Succeeded,
                summary: text.into(),
            });
        }
        t
    }

    #[test]
    fn a_cut_keeps_whole_characters_and_says_how_much_was_shown() {
        let text = "é".repeat(30);
        let cut = cut_with_hint(&text, 10);
        assert!(cut.starts_with(&"é".repeat(10)));
        assert!(cut.contains("10 of 30 characters shown"), "{cut}");
        assert_eq!(cut_with_hint("short", 10), "short");
    }

    #[test]
    fn a_small_reply_passes_and_a_long_one_is_one_capped_text_block() {
        let small = json!({ "content": [{ "type": "text", "text": "ok" }], "structuredContent": {}, "isError": false });
        assert_eq!(cap_tool_reply(small.clone(), "ok".into()).0, small);

        let long = "x".repeat(MAX_TOOL_REPLY_CHARS * 3);
        let big = json!({ "content": [{ "type": "text", "text": long }], "structuredContent": { "a": long }, "isError": true });
        let (capped, text) = cap_tool_reply(big, long.clone());
        assert!(capped.get("structuredContent").is_none());
        assert_eq!(capped["isError"], json!(true));
        assert_eq!(capped["content"][0]["text"], json!(text));
        assert!(text.contains("truncated"));
        assert!(capped.to_string().chars().count() < MAX_TOOL_REPLY_CHARS + 400);
    }

    #[test]
    fn only_the_newest_image_goes_with_a_request() {
        let prior = vec![turn(1, None), turn(2, None)];
        let messages = build_turn_messages(&prior, "now", None, shot());
        let images = messages.iter().filter(|m| m.screenshot.is_some()).count();
        assert_eq!(images, MAX_IMAGES_PER_REQUEST);
        assert!(messages.last().is_some_and(|m| m.screenshot.is_some()), "the current prompt keeps its image");
    }

    #[test]
    fn old_turns_are_dropped_and_the_latest_get_job_reply_is_kept() {
        let mut prior: Vec<ChatTurn> = (1..=MAX_PRIOR_TURNS as u64 + 3).map(|i| turn(i, None)).collect();
        prior[0] = turn(1, Some("{\"old\":true}"));
        prior[1] = turn(2, Some("{\"latest_job\":true}"));
        let messages = build_turn_messages(&prior, "now", None, None);
        // Each kept turn is a prompt and an answer; then the current prompt.
        assert_eq!(messages.len(), MAX_PRIOR_TURNS * 2 + 1);
        assert!(!messages.iter().any(|m| m.text.ends_with("prompt 1") || m.text == "answer 3"));
        let first = &messages[0];
        assert!(first.text.contains("latest_job"), "{}", first.text);
        assert!(!first.text.contains("\"old\""));
        assert!(first.text.ends_with(&format!("prompt {}", 4)), "{}", first.text);

        // Few turns: nothing is dropped and no context note is added.
        let few = build_turn_messages(&prior[..2], "now", None, None);
        assert_eq!(few.len(), 5);
        assert_eq!(few[0].text, "prompt 1");
    }
}
