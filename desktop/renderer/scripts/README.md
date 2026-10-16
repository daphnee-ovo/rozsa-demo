# 共享 UI 脚本

`core/renderer.ts` 是共享 UI 模块入口，提供 template mount 和 host 生命周期；`core/ports.ts` 规定异步 request、事件订阅清理及单调 revision 提交。Vite 负责将这些 TypeScript 模块转换为浏览器 JavaScript；不维护手写 JavaScript 副本。

Prototype shell 创建 mock adapter，场景数据通过 port 读取 `prototype/mock/`。当前 Electron 产品入口是 `index.html`/`entry.ts`，通过 `ProductClient` 使用受控 preload bridge。单独的 runtime main/sidebar 模块不属于当前打包入口；Rust/Tauri 产品继续从 `crates/rozsa-gui/frontend/` 启动，不加载此目录资源。

`core/shell.ts` 装配宿主选定的 template；`core/template-loader.ts` 加载并克隆 template，原型转发文件通过 `data-ui-template-src` 指向共享 owner。Prototype 的 TypeScript shell 使用同一组模板；Vite 的 renderer 输出用于实际页面加载。主窗口在 `RozsaUIReady` 完成后初始化产品事件与状态。Popover 使用共享 template、样式和 component；宿主提供选项数据与保存操作。

request 的 AbortSignal 取消调用方接收结果；若宿主没有取消协议，取消接收不代表后台操作停止。产品 abort 必须显式发出产品操作。异常保持拒绝；dispose 撤销所有订阅并拒绝仍在等待的 request。revision 只在验证与 render 成功后提交。

## Related Docs

- [GUI 架构](../../../docs/gui/ARCHITECTURE.md)
- [共享样式](../styles/README.md)
