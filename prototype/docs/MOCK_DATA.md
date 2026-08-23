# Mock 数据

原型 mock 位于 `prototype/mock/`，只供 `scripts/adapters/mock-adapter.js` 读取。runtime、Rust、Tauri 配置和应用包不得 import、fetch 或 fallback 到这些文件。

| 目录/文件 | 含义 | 使用场景 |
| --- | --- | --- |
| `sessions/empty.json`、`sessions/complete.json` | 空会话与完整会话 | empty、tool、settings、complete |
| `messages/conversation.json` | 对话消息 | application scene |
| `tools/call-states.json` | tool call 状态 | complete、tool、permission |
| `permissions/request.json` | permission 请求 | permission、notifications |
| `questions/request.json` | AskUserQuestion 请求 | ask-user-question |
| `settings/appearance.json`、`settings/permissions.json` | settings 输入 | settings scenes |
| `notifications/states.json` | notification 状态 | notifications、Dev Flow |
| `dev-flow/runtime.json` | Dev Flow runtime payload | dev-flow scenes |
| `responses/success.json`、`error.json`、`cancel.json` | 可重复结果 | permission/question/flow |

Scene registry 只引用稳定 mock ID；大段数据不复制到 scene HTML 或 behavior script。字段形状尽量接近真实 GUI 输入，但 mock 失败必须保持可见错误，而不是隐式空对象。
