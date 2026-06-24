#!/usr/bin/env bash
# Smoke test for scripts/lib/errors.sh step-aware diagnostics.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="${SCRIPT_DIR}/../lib/errors.sh"

step_output="$(
  # shellcheck source=../lib/errors.sh
  source "${LIB}"
  sk8s_step "format check" "running cargo fmt"
)"

if ! grep -q '\[format check\]' <<<"${step_output}"; then
  echo "expected step banner in sk8s_step output" >&2
  exit 1
fi

fail_output="$(
  # shellcheck source=../lib/errors.sh
  source "${LIB}"
  SK8S_STEP="lint"
  sk8s_fail "clippy reported errors" "Run 'make lint'"
)" 2>&1 || true

if ! grep -q 'ERROR \[lint\]: clippy reported errors' <<<"${fail_output}"; then
  echo "expected step name in sk8s_fail output" >&2
  exit 1
fi

if ! grep -q "Hint: Run 'make lint'" <<<"${fail_output}"; then
  echo "expected hint in sk8s_fail output" >&2
  exit 1
fi

echo "error diagnostics smoke test passed"
