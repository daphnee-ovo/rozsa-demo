#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

node --test crates/rozsa-gui/js-tests/*.test.js
cargo test -p rozsa-gui \
  --test frontend_stylesheet_structure_test \
  --test theme_surface \
  --test frontend_scene_identity \
  --test frontend_scene_continuity \
  --test gui_runtime_views_test \
  --test queue_steering_ui_test \
  --test thinking_display_test \
  --test diff_review_test \
  --test turn_diff_render_contract_test \
  --test permission_runtime_test \
  --test permission_queue_test \
  --test permission_hint_test \
  --test ask_user_question_runtime_test \
  --test transient_popup_dismissal_test \
  --test appearance_settings_test \
  --test settings_capabilities \
  --test permission_display_test \
  --test model_switch_test \
  --test key_bindings \
  --test notification_center_test \
  --test notification_position_test \
  --test rate_limit_sidebar_test \
  --test frontend_platform_fallback \
  --test dev_flow_runtime_wiring_test \
  --test dev_flow_sidebar_test \
  --test dev_flow_settings_test \
  --test session_event_routing_test \
  --test targeted_event_routing \
  --test native_split_acceptance \
  --test gui_scene_routing_test \
  --test native_window_behavior \
  --test file_drag_test \
  --test session_fork_gui_test \
  --test prototype_coverage_inventory_test \
  --test gui_coding_agent_e2e \
  --no-fail-fast
