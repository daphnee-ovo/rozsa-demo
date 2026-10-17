---
source: test
nums: 1
---

- [x] ISSUE-I009：Test TASK-T080 fail:(node:14672) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
  - severity: P1
  - location：test.ci run 1
  - description：(node:14672) [DEP0205] DeprecationWarning: `module.register()` is deprecated. Use `module.registerHooks()` instead.
    (Use `node --trace-deprecation ...` to show where the warning was created)
    /Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64
        if (!diagnosticSeen) { clearTimeout(timeout); reject(new Error(`Packaged app exited before startup diagnostic (${code}):\n${logs}`)); }
                                                             ^
    
    Error: Packaged app exited before startup diagnostic (null):
    sandbox_extension_issue_file_to_process failed for /var/folders/t2/59n19l55799_1z4nfpkg9qyc0000gn/T/rozsa-package-Rt7yC3/Rózsa.app: 1 (Operation not permitted)
    [1017/150346.888690:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150346.889179:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [1017/150346.896785:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150346.896960:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150347.349294:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/150347.439325:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150347.439686:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150347.627050:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/150347.695031:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150347.695194:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150347.947690:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/150348.013404:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150348.013592:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150348.282295:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/150348.357017:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150348.357221:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150348.601934:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/150348.673003:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150348.673172:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150348.945765:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/150349.039841:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150349.040028:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150349.350658:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/150349.427991:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150349.428259:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150349.762231:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/150349.886421:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150349.886628:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150350.269284:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [1017/150350.344219:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150350.344405:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150350.560313:ERROR:content/browser/network_service_instance_impl.cc:650] Network service crashed or was terminated, restarting service.
    [1017/150350.629422:ERROR:base/i18n/icu_util.cc:182] icudtl.dat not found in bundle
    [1017/150350.629696:ERROR:base/i18n/icu_util.cc:237] Invalid file descriptor to ICU data received.
    [14674:1017/150350.906387:ERROR:content/browser/gpu/gpu_process_host.cc:1035] GPU process exited unexpectedly: exit_code=5
    [14674:1017/150350.906658:FATAL:content/browser/gpu/gpu_data_manager_impl_private.cc:417] GPU process isn't usable. Goodbye.
    
        at ChildProcess.<anonymous> (/Users/xinyue/ballad/rozsa-demo/desktop/tests/package-resources.test.ts:64:58)
        at ChildProcess.wrapper (node:events:639:12)
        at ChildProcess.emit (node:events:514:20)
        at ChildProcess._handle.onexit (node:internal/child_process:323:12)
    
    Node.js v26.10.0
  - reproduce：npm --prefix desktop/tests exec -- tsx desktop/tests/package-resources.test.ts
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：TASK-T081 fixed the shared packaged-app check: its damaged .app copy now lives under project tmp/migration/install-tmp, framework symlinks stay relative (verbatimSymlinks), and --enable-logging=stderr exposes the expected ENOENT startup diagnostic. The elevated test.ci gate passed.
