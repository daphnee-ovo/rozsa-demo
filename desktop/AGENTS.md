# AGENTS.md in desktop

针对 `refactor/electron-go-migration` 的临时规定(迁移结束后删除)：
- 禁止像时尚单品一样滥用 `migration`、`eletron`
- migration 是过程，如果一个片段、一个文件最终会持久存在，那么不应该带上 `migration` 字眼；如果是临时存在，只服务于 `migration` 这一短期过程的，那么文件应该放入 `tmp/`
- 类似的， 组件、服务等应该直接以其作用命名，不用 `eletron` 做语焉不详的命名前缀

- 报错信息展示: **报错信息禁止在界面上裸露显示，必须走应用错误通知渠道、控制台输出**。可用`devtools/check-error-display.sh` 快速检查。

## GUI

### 设计来源与优先级
- MUST read `DESIGN.md`  and follow it before making any UI/UX modifications.
- Treat `docs/gui/UI_USAGE_GUIDELINES.md`, `docs/gui/ARCHITECTURE.md`, `docs/gui/TERMINOLOGY.md`, and root `prototype/` as the GUI design sources.
- If the prototype conflicts with an explicit user requirement, the user requirement wins. Obtain approval before modifying the prototype to restore product/prototype consistency.
- Shared prototype CSS and JavaScript may be extracted when doing so improves scene reuse without changing behavior.
- **New GUI elements must preserve the established visual and interaction language**.
- GUI components that interact with the operating system, such as native file pickers, must support macOS, Linux, and Windows in addition to the cross-platform requirements above.


### Icon

- All UI icons MUST use the Lucide icon library to maintain consistent stroke styles, proportions, and visual appearance. Do not mix icon libraries or create custom functional icons.

### 层级与模糊效果

- 模糊(Gaussian blur) 只用于表达层级（浮层、侧栏、弹窗背后的内容）
- 模糊层上的文字必须保证对比度、可读性。
- 模糊层不得嵌套叠加

### **布局（硬性要求）**

- 子元素不得超出所属容器，除非所属容器是显式滚动容器
- 同级流式元素不得重叠；文字不得被遮挡或与其他文字重叠
- 有意的重叠（角标、浮层）必须使用定位并在注释中说明原因
- 有背景或边框的容器：上下 padding 相等，左右 padding 相等
- 图片、图标、输入框设置 max-width: 100%，不得固定宽度大于容器。
- 内容不得贴边，到边缘的距离不小于`--space-1`
- 颜色、字号、间距、圆角只用 tokens，不写硬编码值
- 窗口最小尺寸【宽x高】下，所有页面必须完整可用
- 禁止用 overflow: hidden 藏溢出、用 !important 或固定像素硬凑

### 组件

- 新建前先查 `desktop/render/` 现有组件，能复用则复用
- 小组件持有样式和 padding，大组件只组装与布局，不写样式细节
- 修改基础组件时，必须列出所有使用处并说明影响

### 交付前自检（必须执行并汇报）
1. 启动应用，在【最小 / 常用 / 大屏】三个尺寸下截图并打开查看
2. 逐条核对上述"布局正确性"规则，每条写结论和依据（截图路径）
3. 没检查的项目写"未检查"，不得省略；没有证据不得写"已完成"


## Performance

- Eletron 需做裁剪，裁剪掉本项目不需要的功能和内容。需要做到两个优化：
  - 运行性能优化：启动速度快，运行流畅，占用内存少
  - 空间体积优化

## Test

### Write Test

新增加的测试代码同时满足项目根目录下`AGENTS.md`相应的规定以及以下条件。

- 业务逻辑放在不依赖 electron 的模块里，用 Vitest 测试
- IPC 通过共享 schema 测契约和非法输入
- 组件测试按角色/文本查询，断言用户可见结果；禁止快照测试

## macOS packaged-app 检查

- 完成门禁应提权运行。`ROZSA_TEST_APP_BUNDLE` 只指定源包；测试副本位于项目 `tmp/install-tmp`。复制 `.app` 时保留 framework 相对链接，并用 `--enable-logging=stderr` 捕获启动诊断。
