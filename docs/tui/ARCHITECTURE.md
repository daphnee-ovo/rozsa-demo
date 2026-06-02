# rozsa-tui
## 三层通信架构

```
┌─────────────────────────────────────────────┐
│ 核心后端 (packages/coding-agent/src/core/)    │
│ SessionManager · AgentSession · Runtime       │
└─────────────────────┬───────────────────────┘
                      │ 内部 API 调用
┌─────────────────────▼───────────────────────┐
│ 桥接层 (packages/coding-agent/src/modes/native/)│
│ native-mode.ts · protocol.ts                  │
│                                               │
│ 职责：                                        │
│ · 将核心后端数据映射为 NativeSessionEntry 等   │
│ · backend-only 模式监听 Rust 创建的 socket     │
│ · 通过 Unix Socket 发送 JSON Lines 消息        │
│ · 接收前端请求并调用核心 API                   │
└─────────────────────┬───────────────────────┘
                      │ Unix Socket (JSON Lines)
┌─────────────────────▼───────────────────────┐
│ 前端 (crates/rozsa-tui/src/)                  │
│ Rust TUI (ratatui + crossterm)                │
│                                               │
│ 职责：                                        │
│ · 启动 TS backend-only 子进程                  │
│ · 纯展示 + 交互逻辑                           │
│ · 本地排序/过滤/树构建（不直接读文件）          │
│ · 渲染到终端                                  │
└─────────────────────────────────────────────┘
```

## 协议消息清单

### Client → Server (Rust → TS)

| 消息 | 字段 | 用途 |
|------|------|------|
| submit | text, images? | 发送用户输入 |
| abort | — | 中止流式输出 |
| follow_up | text, images? | 追加提问 |
| steer | text, images? | 引导重写 |
| compact | — | 手动压缩 |
| cycle_model | direction | 切换模型 |
| cycle_thinking | — | 切换思考级别 |
| cycle_edit_mode | — | 切换编辑模式 |
| list_sessions | scope? | 请求会话列表（current/all） |
| list_models | — | 请求模型列表 |
| switch_session | path | 切换会话 |
| delete_session | path | 删除会话 |
| rename_session | path, name | 重命名会话 |
| switch_model | id | 切换到指定模型 |
| switch_agent | id | 切换子代理 |
| dialog_response | id, value?, confirmed?, cancelled? | 对话框回复 |
| permission_response | id, choice, trustKey? | 权限回复 |
| autocomplete_request | id, text, cursor, force | 补全请求 |
| bash | command | 执行命令 |
| exit | — | 退出 |

### Server → Client (TS → Rust)

| 消息 | 字段 | 用途 |
|------|------|------|
| state | state | 全量 UI 状态推送 |
| dialog | id, kind, title, ... | 弹出对话框 |
| notify | level, message | 通知 |
| set_title | title | 终端标题 |
| set_input | text | 覆盖输入框 |
| autocomplete | id, prefix, items | 补全结果 |
| permission | prompt | 权限请求 |
| graph | nodes | 会话历史图 |
| sessions | entries, currentSessionPath | 会话列表 |
| session_deleted | path, method, error? | 删除结果 |
| models | entries | 模型列表 |
| retry | seconds, reason | 重试倒计时 |
| compacting | active | 压缩状态 |
| shutdown | — | 关闭 |

## 数据流向

1. 用户操作（键盘/命令） → Rust 前端处理
2. 需要后端数据时 → 通过 socket 发送请求消息
3. 后端处理后 → 发送响应/推送消息
4. Rust 前端接收 → 更新本地状态 → 重新渲染

本地可完成的操作（排序、过滤、树构建）不需要请求后端。
