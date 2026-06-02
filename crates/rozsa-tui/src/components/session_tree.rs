// components/session_tree.rs — 会话树结构
//
// Internal Framework:
// session_tree.rs
// └── build_and_flatten()  构建并扁平化会话树
//
// Related Docs:
// - [TUI Design](../../../docs/rozsa_framework.md#rozsa-tui--ratatui-终端前端)

// session_tree.rs
// ├── TreeNode             # 树节点
// ├── build_session_tree() # 根据 parent_session_path 构建树
// ├── flatten_tree()       # DFS 展平为 FlatSessionNode
// └── build_and_flatten()  # 入口：构建+展平
//
// Related: [session-selector.ts L208-264](../../coding-agent/src/modes/interactive/components/session-selector.ts)

use std::collections::HashMap;

use super::session_selector::{FlatSessionNode, SessionEntry};

struct TreeNode {
    entry_index: usize,
    children: Vec<TreeNode>,
}

/// 根据 parent_session_path 构建树，返回根节点列表（按 last_modified 降序）
fn build_session_tree(entries: &[SessionEntry], indices: &[usize]) -> Vec<TreeNode> {
    // 先建 path → index 映射（仅限参与者）
    let mut path_to_idx: HashMap<&str, usize> = HashMap::new();
    for &i in indices {
        path_to_idx.insert(&entries[i].path, i);
    }

    // 建 children map
    let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut has_parent: Vec<bool> = vec![false; entries.len()];

    for &i in indices {
        if let Some(ref parent_path) = entries[i].parent_session_path {
            if let Some(&parent_idx) = path_to_idx.get(parent_path.as_str()) {
                if parent_idx != i {
                    children_map.entry(parent_idx).or_default().push(i);
                    has_parent[i] = true;
                }
            }
        }
    }

    // 对每个节点的子节点按 last_modified 降序排列
    for children in children_map.values_mut() {
        children.sort_by(|a, b| entries[*b].last_modified.cmp(&entries[*a].last_modified));
    }

    // 根节点：没有有效 parent 的
    let mut roots: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|&i| !has_parent[i])
        .collect();
    roots.sort_by(|a, b| entries[*b].last_modified.cmp(&entries[*a].last_modified));

    // 递归构建树
    fn build_node(idx: usize, children_map: &HashMap<usize, Vec<usize>>) -> TreeNode {
        let children = children_map
            .get(&idx)
            .map(|cs| cs.iter().map(|&c| build_node(c, children_map)).collect())
            .unwrap_or_default();
        TreeNode {
            entry_index: idx,
            children,
        }
    }

    roots
        .iter()
        .map(|&i| build_node(i, &children_map))
        .collect()
}

/// DFS 展平树为 Vec<FlatSessionNode>
fn flatten_tree(roots: &[TreeNode]) -> Vec<FlatSessionNode> {
    let mut result = Vec::new();

    fn walk(
        node: &TreeNode,
        depth: usize,
        ancestor_continues: &[bool],
        is_last: bool,
        result: &mut Vec<FlatSessionNode>,
    ) {
        result.push(FlatSessionNode {
            entry_index: node.entry_index,
            depth,
            is_last,
            ancestor_continues: ancestor_continues.to_vec(),
        });

        for (i, child) in node.children.iter().enumerate() {
            let child_is_last = i == node.children.len() - 1;
            let continues = if depth > 0 { !is_last } else { false };
            let mut child_ancestors = ancestor_continues.to_vec();
            child_ancestors.push(continues);
            walk(child, depth + 1, &child_ancestors, child_is_last, result);
        }
    }

    for (i, root) in roots.iter().enumerate() {
        let is_last = i == roots.len() - 1;
        walk(root, 0, &[], is_last, &mut result);
    }

    result
}

/// 入口函数：构建树并展平
pub fn build_and_flatten(entries: &[SessionEntry], indices: &[usize]) -> Vec<FlatSessionNode> {
    let roots = build_session_tree(entries, indices);
    flatten_tree(&roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::session_selector::SessionEntry;

    fn make_entry(path: &str, parent: Option<&str>, modified: &str) -> SessionEntry {
        SessionEntry {
            path: path.to_string(),
            name: None,
            first_message: format!("msg for {path}"),
            cwd: "/tmp".to_string(),
            message_count: 1,
            last_modified: modified.to_string(),
            parent_session_path: parent.map(|s| s.to_string()),
            all_messages_text: String::new(),
        }
    }

    #[test]
    fn test_flat_no_parents() {
        let entries = vec![
            make_entry("/a", None, "2026-05-29T03:00:00Z"),
            make_entry("/b", None, "2026-05-29T02:00:00Z"),
            make_entry("/c", None, "2026-05-29T01:00:00Z"),
        ];
        let indices = vec![0, 1, 2];
        let flat = build_and_flatten(&entries, &indices);
        assert_eq!(flat.len(), 3);
        // 按 last_modified 降序
        assert_eq!(flat[0].entry_index, 0);
        assert_eq!(flat[1].entry_index, 1);
        assert_eq!(flat[2].entry_index, 2);
        assert_eq!(flat[0].depth, 0);
    }

    #[test]
    fn test_parent_child_tree() {
        let entries = vec![
            make_entry("/parent", None, "2026-05-29T03:00:00Z"),
            make_entry("/child1", Some("/parent"), "2026-05-29T02:00:00Z"),
            make_entry("/child2", Some("/parent"), "2026-05-29T01:00:00Z"),
        ];
        let indices = vec![0, 1, 2];
        let flat = build_and_flatten(&entries, &indices);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].entry_index, 0);
        assert_eq!(flat[0].depth, 0);
        assert_eq!(flat[1].entry_index, 1);
        assert_eq!(flat[1].depth, 1);
        assert!(!flat[1].is_last);
        assert_eq!(flat[2].entry_index, 2);
        assert_eq!(flat[2].depth, 1);
        assert!(flat[2].is_last);
    }

    #[test]
    fn test_orphan_parent_becomes_root() {
        let entries = vec![
            make_entry("/a", Some("/nonexistent"), "2026-05-29T02:00:00Z"),
            make_entry("/b", None, "2026-05-29T01:00:00Z"),
        ];
        let indices = vec![0, 1];
        let flat = build_and_flatten(&entries, &indices);
        assert_eq!(flat.len(), 2);
        // /a 有无效 parent，归为根节点
        assert_eq!(flat[0].depth, 0);
        assert_eq!(flat[1].depth, 0);
    }

    #[test]
    fn test_nested_three_levels() {
        let entries = vec![
            make_entry("/root", None, "2026-05-29T03:00:00Z"),
            make_entry("/mid", Some("/root"), "2026-05-29T02:00:00Z"),
            make_entry("/leaf", Some("/mid"), "2026-05-29T01:00:00Z"),
        ];
        let indices = vec![0, 1, 2];
        let flat = build_and_flatten(&entries, &indices);
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[0].depth, 0); // /root
        assert_eq!(flat[1].depth, 1); // /mid
        assert_eq!(flat[2].depth, 2); // /leaf
    }
}
