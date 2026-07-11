// kill_ring.rs
//
// Internal Framework:
// kill_ring.rs
// ├── KillRing
// │   ├── push()        — 添加文本（支持累积模式）
// │   ├── peek()        — 查看最近条目
// │   ├── rotate()      — 旋转环（yank-pop 循环）
// │   └── len()         — 条目数量
// └── PushOpts          — push 操作选项
//
// Related Docs:
// - [Task T002](../../dev-doc/refactor/tui/task/task_2026-05-28_1.md)

/// Emacs 风格 Kill Ring
#[derive(Clone, Debug)]
pub struct KillRing {
    ring: Vec<String>,
}

pub struct PushOpts {
    /// 累积时文本添加到前面还是后面
    pub prepend: bool,
    /// 是否累积到最近条目（连续 kill 操作合并）
    pub accumulate: bool,
}

impl KillRing {
    pub fn new() -> Self {
        Self { ring: Vec::new() }
    }

    /// 向 kill ring 添加文本
    pub fn push(&mut self, text: &str, opts: PushOpts) {
        if text.is_empty() {
            return;
        }
        if opts.accumulate && !self.ring.is_empty() {
            let last = self.ring.last_mut().unwrap();
            if opts.prepend {
                *last = format!("{text}{last}");
            } else {
                last.push_str(text);
            }
        } else {
            self.ring.push(text.to_string());
        }
    }

    /// 查看最近的 kill ring 条目
    pub fn peek(&self) -> Option<&str> {
        self.ring.last().map(|s| s.as_str())
    }

    /// 旋转环：最后一个移到最前（用于 yank-pop 循环）
    pub fn rotate(&mut self) {
        if self.ring.len() > 1 {
            let last = self.ring.pop().unwrap();
            self.ring.insert(0, last);
        }
    }

    pub fn len(&self) -> usize {
        self.ring.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

impl Default for KillRing {
    fn default() -> Self {
        Self::new()
    }
}

/// 追踪最近操作类型（用于 kill 累积和 yank-pop 判断）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastAction {
    Kill,
    Yank,
    TypeWord,
}
