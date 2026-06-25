# Changelog


## 2026-06-05
- 19:58 feat: Settings Dialog 增加 Provider Tabs 分类筛选

## 2026-06-23
- 12:58 Merge branch 'refactor/core' — rozsa-core agent loop migration
- 17:56 Merge branch 'refactor/app' — rozsa-app full runtime implementation
- 18:10 feat: Implement NativeBackend in rozsa-tui
- 19:04 refactor: Unify duplicated types (Model, SessionEntry, ThinkingLevel)

## 2026-06-25
- 16:29 fix: NativeBackend 多项 bug 修复 + settings 持久化 + 回归测试
- 16:30 refactor: 清理死代码 + 统一类型 + 对接权限/压缩/扩展/技能系统
- 18:02 fix: ISSUE-I001：PermissionPolicy 未从 pre_tool_use hook 调用 — tool 执行无守卫
- 18:23 fix: ISSUE-I004：SkillRegistry 未接入 TUI/AgentSession
