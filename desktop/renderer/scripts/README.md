# 共享 UI 脚本

`core/renderer.js` 是两个宿主共同的模块入口，提供 template mount 和 host 生命周期；`core/ports.js` 规定异步 request、事件订阅清理及单调 revision 提交。

原型 shell 创建 mock adapter，场景数据通过 port 读取 `prototype/mock/`。生产 main/sidebar entry 创建 Tauri adapter，保留现有 WebView bridge、产品状态、handler 和 IPC 命令；adapter 不创建 DOM、不选择 CSS、不维护产品状态副本。

`core/shell.js` 装配宿主选定的 template；`core/template-loader.js` 加载并克隆 template，原型转发文件通过 `data-ui-template-src` 指向共享 owner。主窗口在 `RozsaUIReady` 完成后初始化产品事件与状态。Popover 使用共享 template、样式和 component；宿主提供选项数据与保存操作。

request 的 AbortSignal 取消调用方接收结果；Tauri invoke 没有取消协议，取消接收不代表后台操作停止。产品 abort 必须显式发出原有操作。异常保持拒绝；dispose 撤销所有订阅并拒绝仍在等待的 request。revision 只在验证与 render 成功后提交。

## Related Docs

- [GUI 架构](../../../docs/gui/ARCHITECTURE.md)
- [共享样式](../styles/README.md)
