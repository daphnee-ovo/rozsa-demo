---
source: other
nums: 1
---

- [x] ISSUE-I004：补充纯只读 Bash 命令白名单
  - severity: P0
  - location：crates/rozsa-app/src/permissions/mod.rs:357
  - description：在现有 head、tail、cat、grep、sort 基础上补充 pwd、ls、basename、dirname、realpath、readlink、stat、file、wc、diff、cmp、comm、cut、tr、uniq、strings、od、xxd；继续限制 workspace 路径和现有危险参数。
  - reproduce：在 OnRequest 模式执行上述纯只读 Bash 命令，当前均触发 permission；预期在安全参数和 workspace 路径下自动放行。
  - fix：补充 pwd、ls、basename、dirname、realpath、readlink、stat、file、wc、diff、cmp、comm、cut、tr、uniq、strings、od、xxd；继续执行 workspace 路径检查，并限制 diff/realpath/file 的内联路径选项。
  - files_modify: [crates/rozsa-app/src/permissions/mod.rs, crates/rozsa-app/tests/permission_safe_commands_test.rs]
  - files_create: []
