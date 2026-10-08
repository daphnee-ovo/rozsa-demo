---
source: test
nums: 6
---

- [x] ISSUE-I008：Test TASK-T022 fail:Could not find 'crates/rozsa-gui/js-tests/shared-suspension.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/shared-suspension.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/shared-suspension.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/shared-suspension.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Node 文件实际存在。dev-flow JS adapter 切换到 package_root 后仍使用仓库相对路径；T022 改用 devtools/test-sidebar-dev-flow.sh 显式定位仓库并执行全部专项 Node/Cargo 测试，dow task done T022 已成功。
  - files_modify: [crates/rozsa-gui/js-tests/shared-suspension.test.js]
  - files_create: []
- [x] ISSUE-I009：Test TASK-T022 fail:Could not find 'crates/rozsa-gui/js-tests/shared-sidebar.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/shared-sidebar.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/shared-sidebar.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/shared-sidebar.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Node 文件实际存在。dev-flow JS adapter 切换到 package_root 后仍使用仓库相对路径；T022 改用 devtools/test-sidebar-dev-flow.sh 显式定位仓库并执行全部专项 Node/Cargo 测试，dow task done T022 已成功。
  - files_modify: [crates/rozsa-gui/js-tests/shared-sidebar.test.js]
  - files_create: []
- [x] ISSUE-I010：Test TASK-T022 fail:Could not find 'crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Node 文件实际存在。dev-flow JS adapter 切换到 package_root 后仍使用仓库相对路径；T022 改用 devtools/test-sidebar-dev-flow.sh 显式定位仓库并执行全部专项 Node/Cargo 测试，dow task done T022 已成功。
  - files_modify: [crates/rozsa-gui/js-tests/shared-ui-host-contract.test.js]
  - files_create: []
- [x] ISSUE-I011：Test TASK-T022 fail:Could not find 'crates/rozsa-gui/js-tests/dev-flow-sidebar.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/dev-flow-sidebar.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/dev-flow-sidebar.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/dev-flow-sidebar.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Node 文件实际存在。dev-flow JS adapter 切换到 package_root 后仍使用仓库相对路径；T022 改用 devtools/test-sidebar-dev-flow.sh 显式定位仓库并执行全部专项 Node/Cargo 测试，dow task done T022 已成功。
  - files_modify: [crates/rozsa-gui/js-tests/dev-flow-sidebar.test.js]
  - files_create: []
- [x] ISSUE-I012：Test TASK-T022 fail:Could not find 'crates/rozsa-gui/js-tests/dev-flow-settings.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/dev-flow-settings.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/dev-flow-settings.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/dev-flow-settings.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Node 文件实际存在。dev-flow JS adapter 切换到 package_root 后仍使用仓库相对路径；T022 改用 devtools/test-sidebar-dev-flow.sh 显式定位仓库并执行全部专项 Node/Cargo 测试，dow task done T022 已成功。
  - files_modify: [crates/rozsa-gui/js-tests/dev-flow-settings.test.js]
  - files_create: []
- [x] ISSUE-I013：Test TASK-T022 fail:Could not find 'crates/rozsa-gui/js-tests/dev-flow-tool-presentation.test.js'
  - severity: P1
  - location：crates/rozsa-gui/js-tests/dev-flow-tool-presentation.test.js
  - description：Could not find 'crates/rozsa-gui/js-tests/dev-flow-tool-presentation.test.js'
  - reproduce：node --test 'crates/rozsa-gui/js-tests/dev-flow-tool-presentation.test.js'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：Node 文件实际存在。dev-flow JS adapter 切换到 package_root 后仍使用仓库相对路径；T022 改用 devtools/test-sidebar-dev-flow.sh 显式定位仓库并执行全部专项 Node/Cargo 测试，dow task done T022 已成功。
  - files_modify: [crates/rozsa-gui/js-tests/dev-flow-tool-presentation.test.js]
  - files_create: []
