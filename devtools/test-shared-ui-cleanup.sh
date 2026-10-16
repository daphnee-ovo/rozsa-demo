#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

node --test crates/rozsa-gui/js-tests/*.test.js
cargo test -p rozsa-gui \
  --test dev_flow_runtime_wiring_test \
  --test dev_flow_sidebar_test \
  --test dev_flow_settings_test \
  --test session_event_routing_test \
  --test targeted_event_routing \
  --test native_split_acceptance \
  --test frontend_platform_fallback \
  --test rate_limit_sidebar_test \
  --test frontend_stylesheet_structure_test \
  --test prototype_scene_ownership_test \
  --test prototype_scene_fixture_test \
  --test prototype_layout_parity_test \
  --test native_window_behavior \
  --test gui_layout_polish_test \
  --test session_title_test
