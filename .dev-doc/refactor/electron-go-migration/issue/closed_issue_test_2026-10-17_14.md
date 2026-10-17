---
source: test
nums: 1
---

- [x] ISSUE-I014：Test TASK-T019 fail:(node:17015) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：backend/internal/core/agent/agent_test.go
  - description：(node:17015) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
    (Use `node --trace-deprecation ...` to show where the warning was created)
    /Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64
        if (!diagnosticSeen) { clearTimeout(timeout); reject(new Error(`Packaged app exited before startup diagnostic (${code}):\n${logs}`)); }
                                                             ^
    
    Error: Packaged app exited before startup diagnostic (null):
    [1017/181225.320398:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181225.321758:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [1017/181225.320414:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181225.321912:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181225.929634:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181226.113799:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181226.114122:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181226.877053:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181227.029578:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181227.029893:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181227.614688:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181227.729941:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181227.730289:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181228.013728:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181228.091194:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181228.091473:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181228.478488:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181228.601993:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181228.605797:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181228.983829:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181229.064253:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181229.064570:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181229.528068:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181229.663829:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181229.664079:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181230.437556:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181230.585173:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181230.585430:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181231.469227:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181231.557127:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181231.557726:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181231.969257:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181232.071032:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181232.071370:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181232.446054:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181232.551950:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181232.552220:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [17020:1017/181232.980598:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [17020:1017/181232.980816:FATAL:content/browser/gpu/gpu_data_manager_impl_private.cc:417] GPU process isn't usable. Goodbye.
    
        at ChildProcess.<anonymous> (/Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64:58)
        at ChildProcess.wrapper (node:events:639:12)
        at ChildProcess.emit (node:events:514:20)
        at ChildProcess._handle.onexit (node:internal/child_process:323:12)
    
    Node.js v26.10.0
  - reproduce：npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：TASK-T081 fixed the shared packaged-app check: its damaged .app copy now lives under project tmp/migration/install-tmp, framework symlinks stay relative (verbatimSymlinks), and --enable-logging=stderr exposes the expected ENOENT startup diagnostic. The elevated test.ci gate passed.
  - files_modify: [backend/internal/core/agent/agent_test.go, backend/internal/core/tools/tools_test.go, backend/internal/core/hooks/hooks_test.go]
  - files_create: []
