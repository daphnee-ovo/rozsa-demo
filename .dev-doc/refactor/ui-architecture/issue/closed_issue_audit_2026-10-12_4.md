---
source: audit
nums: 1
---

- [x] ISSUE-I017：修复 Gallery 模块入口被残留 script 包裹导致交互失效
  - severity: P1
  - location：prototype/gallery/visual-direction.html:241
  - description：真实浏览器报 SyntaxError: Unexpected token <；残留 script opening tag将外部模块入口当成inline JavaScript，Gallery视图与主题按钮无响应。归因：该残留入口存在于5400367切换根原型的提交中。
  - reproduce：打开Gallery，点击Active或Appearance，aria-selected仍为Empty；浏览器console出现SyntaxError。
  - fix：删除残留script opening tag，保留唯一模块入口。Node回归在修复前报Unexpected token <，修复后通过；真实Browser已验证Empty/Active/Appearance与Light到Dark切换。
  - files_modify: [prototype/gallery/visual-direction.html]
  - files_create: [crates/rozsa-gui/js-tests/prototype-gallery.test.js]
