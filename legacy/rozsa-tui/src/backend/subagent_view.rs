// backend/subagent_view.rs — sidebar 同步查询子代理状态的窄接口
//
// sidebar 的渲染在 ratatui 同步循环中执行，不能 .await。
// SubagentView 用 try_lock 风格的查询暴露 SubagentManager 的当前快照。

use rozsa_app::subagent::SubagentInfo;

/// 同步、非阻塞地查询子代理状态。
/// 实现可以在锁被持有时返回空 / None，sidebar 当作"暂无信息"渲染。
pub trait SubagentView: Send + Sync {
    /// Best-effort 列出当前子代理。锁被占用时返回空列表。
    fn list_subagents_sync(&self) -> Vec<SubagentInfo>;
    /// 当前正在查看的子代理 id（None = 主 session）。
    fn viewing_subagent_id_sync(&self) -> Option<String>;
}
