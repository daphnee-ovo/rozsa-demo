# AGENTS.md

- Eletron 需做裁剪，裁剪掉本项目不需要的功能和内容。需要做到两个优化：
  - 运行性能优化：启动速度快，运行流畅，占用内存少
  - 空间体积优化

针对 `refactor/electron-go-migration` 的临时规定(迁移结束后删除)：
- 禁止像时尚单品一样滥用 `migration`、`eletron`
- migration 是过程，如果一个片段、一个文件最终会持久存在，那么不应该带上 `migration` 字眼；如果是临时存在，只服务于 `migration` 这一短期过程的，那么文件应该放入 `tmp/`
- 类似的， 组件、服务等应该直接以其作用命名，不用 `eletron` 做语焉不详的命名前缀

## macOS packaged-app 检查

- 完成门禁应提权运行。`ROZSA_TEST_APP_BUNDLE` 只指定源包；测试副本位于项目 `tmp/install-tmp`。复制 `.app` 时保留 framework 相对链接，并用 `--enable-logging=stderr` 捕获启动诊断。
