#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

node --test \
  desktop/tests/shared-dev-flow.test.js \
  desktop/tests/shared-suspension.test.js \
  desktop/tests/shared-sidebar.test.js \
  desktop/tests/shared-ui-host-contract.test.js \
  desktop/tests/dev-flow-sidebar.test.js \
  desktop/tests/dev-flow-settings.test.js \
  desktop/tests/dev-flow-tool-presentation.test.js

cargo test -p rozsa-gui \
  --test dev_flow_runtime_wiring_test \
  --test dev_flow_sidebar_test \
  --test dev_flow_settings_test \
  --test session_event_routing_test \
  --test targeted_event_routing \
  --test native_split_acceptance \
  --test frontend_platform_fallback \
  --test rate_limit_sidebar_test \
  --test prototype_stylesheet_extraction_test
