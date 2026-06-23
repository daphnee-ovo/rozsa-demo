use std::future::Future;

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
    let chars = match entry {
        SessionEntry::Message(e) => {
            serde_json::to_string(&e.message).unwrap_or_default().len()
        }
        SessionEntry::Compaction(e) => e.summary.len(),
        SessionEntry::Custom(e) => {
            e.data.as_ref().map_or(0, |v| serde_json::to_string(v).unwrap_or_default().len())
        }
        _ => 0,
    };
    (chars as u64) / 4
}

fn entry_text_content(entry: &SessionEntry) -> Option<String> {
    match entry {
        SessionEntry::Message(e) => {
            Some(serde_json::to_string(&e.message).unwrap_or_default())
        }
        _ => None,
    }
}
