use std::future::Future;

use rozsa_model::types::{ContentBlock, Message};

use crate::session::manager::SessionEntry;

#[derive(Debug, Clone)]
pub struct CompactionTrigger {
    pub threshold_tokens: u64,
    pub target_tokens: u64,
}

impl Default for CompactionTrigger {
    fn default() -> Self {
        Self {
            threshold_tokens: 100_000,
            target_tokens: 20_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompactionPlan {
    pub cut_point_index: usize,
    pub entries_to_remove: Vec<String>,
    pub estimated_tokens_before: u64,
}

#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub summary: String,
    pub removed_count: usize,
}

pub struct CompactionEngine {
    pub trigger: CompactionTrigger,
}

impl CompactionEngine {
    pub fn new(trigger: CompactionTrigger) -> Self {
        Self { trigger }
    }

    pub fn should_compact(&self, context_tokens: u64) -> bool {
        context_tokens > self.trigger.threshold_tokens
    }

    pub fn prepare(&self, entries: &[SessionEntry]) -> Option<CompactionPlan> {
        if entries.is_empty() {
            return None;
        }

        let total_tokens: u64 = entries.iter().map(|e| estimate_entry_tokens(e)).sum();
        if total_tokens <= self.trigger.threshold_tokens {
            return None;
        }

        let keep_tokens = self.trigger.target_tokens;
        let mut kept = 0u64;
        let mut cut_index = entries.len();

        for (i, entry) in entries.iter().enumerate().rev() {
            kept += estimate_entry_tokens(entry);
            if kept >= keep_tokens {
                cut_index = i;
                break;
            }
        }

        if cut_index == 0 || cut_index >= entries.len() {
            return None;
        }

        // Adjust cut point to avoid breaking tool_use/tool_result pairs.
        let cut_index = adjust_to_safe_boundary(entries, cut_index);
        if cut_index == 0 {
            return None;
        }

        let entries_to_remove: Vec<String> = entries[..cut_index]
            .iter()
            .map(|e| e.id().to_string())
            .collect();

        Some(CompactionPlan {
            cut_point_index: cut_index,
            entries_to_remove,
            estimated_tokens_before: total_tokens,
        })
    }

    pub async fn execute<F, Fut>(
        &self,
        plan: &CompactionPlan,
        entries: &[SessionEntry],
        summarize_fn: F,
    ) -> anyhow::Result<CompactionResult>
    where
        F: FnOnce(String) -> Fut,
        Fut: Future<Output = anyhow::Result<String>>,
    {
        let content_to_summarize: String = entries[..plan.cut_point_index]
            .iter()
            .filter_map(|e| entry_text_content(e))
            .collect::<Vec<_>>()
            .join("\n");

        let summary = summarize_fn(content_to_summarize).await?;

        Ok(CompactionResult {
            summary,
            removed_count: plan.entries_to_remove.len(),
        })
    }
}

impl Default for CompactionEngine {
    fn default() -> Self {
        Self::new(CompactionTrigger::default())
    }
}

fn estimate_entry_tokens(entry: &SessionEntry) -> u64 {
    match entry {
        SessionEntry::Message(e) => match &e.message {
            rozsa_model::types::Message::Assistant(a) if a.usage.output > 0 => {
                a.usage.output
            }
            other => estimate_tokens(&serde_json::to_string(other).unwrap_or_default()),
        },
        SessionEntry::Compaction(e) => estimate_tokens(&e.summary),
        SessionEntry::Custom(e) => {
            e.data.as_ref().map_or(0, |v| {
                estimate_tokens(&serde_json::to_string(v).unwrap_or_default())
            })
        }
        _ => 0,
    }
}

/// Estimate token count: ~4 bytes per token (same heuristic as Codex).
/// UTF-8 encoding naturally weights CJK (3 bytes/char) higher than ASCII (1 byte/char).
fn estimate_tokens(text: &str) -> u64 {
    (text.len() as u64 + 3) / 4
}

/// Adjust cut_index forward (toward 0) so the tail doesn't start mid-tool-pair.
///
/// API constraint: a ToolResult message MUST follow the Assistant message that
/// issued the corresponding ToolCall. Cutting between them causes API errors on resume.
fn adjust_to_safe_boundary(entries: &[SessionEntry], raw_cut: usize) -> usize {
    let mut cut = raw_cut;

    while cut > 0 {
        let Some(SessionEntry::Message(e)) = entries.get(cut) else {
            break;
        };
        match &e.message {
            // If tail starts with ToolResult, pull cut back to include the preceding Assistant.
            Message::ToolResult(_) => {
                cut -= 1;
                continue;
            }
            // If Assistant has ToolCall and the next entry is its ToolResult, keep the pair together.
            Message::Assistant(a)
                if a.content.iter().any(|b| matches!(b, ContentBlock::ToolCall(_))) =>
            {
                let next_is_tool_result = entries.get(cut + 1).is_some_and(|next| {
                    matches!(next, SessionEntry::Message(m) if matches!(m.message, Message::ToolResult(_)))
                });
                if next_is_tool_result {
                    cut -= 1;
                    continue;
                }
            }
            _ => {}
        }
        break;
    }

    cut
}

/// Serialize a message entry for the summarize prompt.
/// Strips image binary data to avoid bloating the summary request.
fn entry_text_content(entry: &SessionEntry) -> Option<String> {
    match entry {
        SessionEntry::Message(e) => {
            if has_image_content(&e.message) {
                Some(serialize_without_images(&e.message))
            } else {
                Some(serde_json::to_string(&e.message).unwrap_or_default())
            }
        }
        _ => None,
    }
}

fn has_image_content(msg: &Message) -> bool {
    let blocks = match msg {
        Message::Assistant(a) => &a.content,
        Message::ToolResult(t) => &t.content,
        Message::User(_) => return false,
    };
    blocks.iter().any(|b| matches!(b, ContentBlock::Image { .. }))
}

fn serialize_without_images(msg: &Message) -> String {
    let mut val = serde_json::to_value(msg).unwrap_or_default();
    strip_image_data(&mut val);
    serde_json::to_string(&val).unwrap_or_default()
}

fn strip_image_data(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            if map.get("type").and_then(|v| v.as_str()) == Some("image") {
                if let Some(data) = map.get_mut("data") {
                    *data = serde_json::Value::String("[image data omitted]".to_string());
                }
            }
            for v in map.values_mut() {
                strip_image_data(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                strip_image_data(v);
            }
        }
        _ => {}
    }
}
