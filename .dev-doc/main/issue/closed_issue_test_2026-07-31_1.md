---
source: test
nums: 1
---

- [x] ISSUE-I001：Test TASK-T002 fail:running 12 tests
  - severity: P1
  - location：crates/rozsa-app/tests/dev_flow_dashboard_test.rs
  - description：running 12 tests
    test non_loopback_or_redirectable_base_urls_are_rejected ... ok
    test oversized_content_length_is_rejected_before_body_read ... ok
    test reconnect_backoff_is_bounded_and_reports_at_the_defined_threshold ... ok
    test missing_required_fields_and_invalid_ids_are_incompatible ... ok
    test invalid_update_preserves_the_last_good_snapshot_as_stale ... ok
    test snapshot_adapter_accepts_unknown_fields_and_uses_only_data_get ... ok
    test sse_supports_comments_crlf_and_complete_update_events ... ok
    test failed_startup_kills_and_reaps_the_owned_child ... FAILED
    test cancellation_interrupts_waiting_for_response_headers ... ok
    test response_header_deadline_is_enforced ... ok
    test stalled_sse_marks_the_last_snapshot_stale ... ok
    test oversized_sse_event_is_rejected ... ok
    
    failures:
    
    ---- failed_startup_kills_and_reaps_the_owned_child stdout ----
    
    thread 'failed_startup_kills_and_reaps_the_owned_child' (1939801) panicked at crates/rozsa-app/tests/dev_flow_dashboard_test.rs:433:49:
    called `Result::unwrap()` on an `Err` value: Os { code: 2, kind: NotFound, message: "No such file or directory" }
    note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
    
    
    failures:
        failed_startup_kills_and_reaps_the_owned_child
    
    test result: FAILED. 11 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.11s
         Running tests/dev_flow_dashboard_test.rs (/Users/xinyue/ballad/rozsa-demo/target/debug/deps/dev_flow_dashboard_test-62d84b3c573dd1ca)
    error: test failed, to rerun pass `--test dev_flow_dashboard_test`
  - reproduce：cargo test --manifest-path '/Users/xinyue/ballad/rozsa-demo/crates/rozsa-app/Cargo.toml' --test 'dev_flow_dashboard_test'
    project_root: /Users/xinyue/ballad/rozsa-demo
  - fix：将子进程 PID 文件等待上限调整为 1 秒，消除慢速调度下的竞态；随后 dev_flow_dashboard_test 12/12 与相关 32 项测试均通过。
  - files_modify: [crates/rozsa-app/tests/dev_flow_dashboard_test.rs]
  - files_create: []
