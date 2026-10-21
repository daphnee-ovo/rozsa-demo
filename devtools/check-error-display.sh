#!/usr/bin/env bash
# List possible error-to-UI source locations in caller-specified files for manual review.
set -u

if (($# == 0)); then
  printf 'Usage: %s <source-path> [source-path ...]\n' "$0" >&2
  exit 2
fi

pattern='catch|diagnostic|\{[^}]*\b(error|reason)\b[^}]*\}|\b(innerHTML|textContent|insertAdjacentHTML)[[:space:]]*='
failed=0
for source in "$@"; do
  if [[ ! -f "$source" ]]; then
    printf 'Missing source file: %s\n' "$source" >&2
    failed=1
    continue
  fi

  printf '\n--- %s ---\n' "$source"
  printf 'Candidates for manual review only; normal notification and status copy is not automatically a violation.\n'
  rg --no-heading -n -i -C 1 "$pattern" -- "$source"
  status=$?
  if ((status == 1)); then
    printf '(no candidate lines)\n'
  elif ((status != 0)); then
    failed=1
  fi
done

exit "$failed"
