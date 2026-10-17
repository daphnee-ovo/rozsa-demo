---
source: test
nums: 1
---

- [x] ISSUE-I012：Test TASK-T019 fail:(node:13884) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：backend/internal/core/agent/agent_test.go
  - description：(node:13884) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
    (Use `node --trace-deprecation ...` to show where the warning was created)
    /Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64
        if (!diagnosticSeen) { clearTimeout(timeout); reject(new Error(`Packaged app exited before startup diagnostic (${code}):\n${logs}`)); }
                                                             ^
    
    Error: Packaged app exited before startup diagnostic (null):
    sandbox_extension_issue_file_to_process failed for /var/folders/t2/59n19l55799_1z4nfpkg9qyc0000gn/T/rozsa-package-cfu9hI/Rózsa.app: 1 (Operation not permitted)
    [1017/180406.492966:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180406.500788:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [1017/180406.492679:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180406.500814:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180407.191008:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/180407.349841:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180407.350153:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180407.759441:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/180407.858264:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180407.858687:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180408.135093:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/180408.208518:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180408.208738:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180408.503691:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/180408.596273:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180408.596645:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180408.912805:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/180408.987315:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180408.987575:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180409.285107:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/180409.355530:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180409.355785:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180409.684002:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/180409.819338:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180409.819604:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180410.203003:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/180410.335380:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180410.335603:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180410.766662:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/180410.849590:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180410.849827:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180411.116488:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/180411.226554:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/180411.226786:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [13892:1017/180411.614253:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [13892:1017/180411.615216:FATAL:content/browser/gpu/gpu_data_manager_impl_private.cc:417] GPU process isn't usable. Goodbye.
    
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
