#!/usr/bin/env bash
# Line coverage gate. Fails below BERTH_COVERAGE_MIN (default 85%).
set -euo pipefail
cd "$(dirname "$0")/.."
minimum="${BERTH_COVERAGE_MIN:-85}"
exec cargo llvm-cov \
  --workspace \
  --locked \
  --features berth-app/test-driver \
  --summary-only \
  --fail-under-lines "$minimum" \
  "$@"
