---
source: test
nums: 1
---

- [x] ISSUE-I013：Test TASK-T019 fail:(node:16680) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：backend/internal/core/agent/agent_test.go
  - description：(node:16680) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
    (Use `node --trace-deprecation ...` to show where the warning was created)
    /Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64
        if (!diagnosticSeen) { clearTimeout(timeout); reject(new Error(`Packaged app exited before startup diagnostic (${code}):\n${logs}`)); }
                                                             ^
    
    Error: Packaged app exited before startup diagnostic (null):
    sandbox_extension_issue_file_to_process failed for /var/folders/t2/59n19l55799_1z4nfpkg9qyc0000gn/T/rozsa-package-doo81c/Rózsa.app: 1 (Operation not permitted)
    [1017/181143.612643:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181143.613140:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [1017/181143.655674:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181143.656215:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181144.109083:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181144.302513:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181144.302867:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181144.615126:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181144.722676:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181144.722872:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181145.215269:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181145.319060:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181145.319331:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181145.768268:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181145.895105:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181145.895348:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181146.417281:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181146.498830:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181146.499076:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181147.720573:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181148.460862:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181148.462950:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181149.027164:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181149.168709:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181149.169251:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181149.447960:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181149.539603:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181149.539863:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181149.818040:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/181149.919502:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181149.919728:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181150.225651:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/181150.342487:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/181150.344132:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [16681:1017/181150.656599:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [16681:1017/181150.657383:FATAL:content/browser/gpu/gpu_data_manager_impl_private.cc:417] GPU process isn't usable. Goodbye.
    
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
