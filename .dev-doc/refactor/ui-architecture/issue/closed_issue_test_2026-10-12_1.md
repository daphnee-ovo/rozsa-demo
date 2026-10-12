---
source: test
nums: 3
---

- [x] ISSUE-I018：Test TASK-T011 fail:Could not find 'crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：文件实际存在；仓库根目录直接 Node 运行及 npm 全套均通过。T011 完成 runner 的相对路径定位失败，任务测试入口改为同文件绝对路径后 dow task done T011 成功；未修改或跳过 JS 测试，未断言 runner 内部根因。
  - files_modify: [crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js]
  - files_create: []
- [x] ISSUE-I019：Test TASK-T011 fail:Could not find 'crates/rozsa-gui/js-tests/shared-ui-source.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/shared-ui-source.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/shared-ui-source.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/shared-ui-source.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：文件实际存在；仓库根目录直接 Node 运行及 npm 全套均通过。T011 完成 runner 的相对路径定位失败，任务测试入口改为同文件绝对路径后 dow task done T011 成功；未修改或跳过 JS 测试，未断言 runner 内部根因。
  - files_modify: [crates/rozsa-gui/js-tests/shared-ui-source.test.js]
  - files_create: []
- [x] ISSUE-I020：Test TASK-T011 fail:Could not find 'crates/rozsa-gui/js-tests/shared-suspension.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/shared-suspension.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/shared-suspension.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/shared-suspension.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：文件实际存在；仓库根目录直接 Node 运行及 npm 全套均通过。T011 完成 runner 的相对路径定位失败，任务测试入口改为同文件绝对路径后 dow task done T011 成功；未修改或跳过 JS 测试，未断言 runner 内部根因。
  - files_modify: [crates/rozsa-gui/js-tests/shared-suspension.test.js]
  - files_create: []
