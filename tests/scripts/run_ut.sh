#!/usr/bin/env bash
# ============================================================================
# run_ut.sh — Unit Test 编排脚本 (UT 层, POSIX 等价物)
#
# 目的: 对 IM1.0 mock 项目跑单元测试 (`cargo test --workspace --lib`),
#       不需要真实 PostgreSQL / im-gateway, 任何时候都可跑。
#
# 用法:
#   bash tests/scripts/run_ut.sh
#   bash tests/scripts/run_ut.sh --filter im_testkit
#   bash tests/scripts/run_ut.sh --skip-build
#
# 返回: 0 = 全部通过; 非 0 = 有失败
# 范围: Git Bash / WSL / macOS / Linux
# 作者: Mavis 接手 agent per DEC-008 (2026-09-20)
# ============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOGS_DIR="${REPO_ROOT}/tests/logs"
TARGET_DIR=""
FILTER=""
SKIP_BUILD="0"
INCLUDE_NOPG="0"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --filter)      FILTER="$2"; shift 2 ;;
        --skip-build)  SKIP_BUILD="1"; shift ;;
        --target-dir)  TARGET_DIR="$2"; shift 2 ;;
        --include-nopg) INCLUDE_NOPG="1"; shift ;;
        -h|--help)
            grep -E '^# 用途|^# 用法' "${BASH_SOURCE[0]}" | head -10
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 64 ;;
    esac
done

mkdir -p "$LOGS_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="${LOGS_DIR}/ut-${STAMP}.log"
SUMMARY_FILE="${LOGS_DIR}/ut-summary-${STAMP}.txt"

log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$LOG_FILE" >&2 ; }

log "[run_ut] repo   = $REPO_ROOT"
log "[run_ut] filter = '$FILTER'"
log "[run_ut] logs   = $LOGS_DIR"

command -v cargo >/dev/null || { echo "cargo not on PATH" >&2; exit 127; }

BUILD_START=$(date +%s)
if [[ "$SKIP_BUILD" == "1" ]]; then
    log "[1/3] cargo build -- SKIPPED (--skip-build)"
else
    log "[1/3] cargo build --workspace --tests"
    if [[ -n "$TARGET_DIR" ]]; then
        export CARGO_TARGET_DIR="$TARGET_DIR"
    fi
    if ! cargo build --workspace --tests 2>&1 | tee "${LOGS_DIR}/ut-build-${STAMP}.log" >&2; then
        log "[1/3] cargo build FAILED"
        exit 1
    fi
fi
BUILD_ELAPSED=$(( $(date +%s) - BUILD_START ))
log "[1/3] build elapsed: ${BUILD_ELAPSED}s"

TEST_START=$(date +%s)
log "[2/3] cargo test --workspace --lib"
TEST_LOG="${LOGS_DIR}/ut-test-${STAMP}.log"
set +e
if [[ -n "$TARGET_DIR" ]]; then
    export CARGO_TARGET_DIR="$TARGET_DIR"
fi
if [[ -n "$FILTER" ]]; then
    cargo test --workspace --lib --no-fail-fast -- "$FILTER" 2>&1 | tee "$TEST_LOG" >&2
else
    cargo test --workspace --lib --no-fail-fast 2>&1 | tee "$TEST_LOG" >&2
fi
TEST_EXIT=$?
set -e
TEST_ELAPSED=$(( $(date +%s) - TEST_START ))
log "[2/3] test  elapsed: ${TEST_ELAPSED}s"
log "[2/3] test  exit  : $TEST_EXIT"

# 2b. (optional) non-PG integration tests
if [[ "$INCLUDE_NOPG" == "1" ]]; then
    log "[2b/3] non-PG integration tests: migration_smoke + im_testkit_smoke"
    NO_PG_LOG="${LOGS_DIR}/ut-nopg-${STAMP}.log"
    if [[ -n "$TARGET_DIR" ]]; then export CARGO_TARGET_DIR="$TARGET_DIR"; fi
    set +e
    cargo test -p im-gateway --test migration_smoke -- --test-threads=1 2>&1 | tee -a "$NO_PG_LOG" >/dev/null
    NOPG1=$?
    cargo test -p im-testkit --test im_testkit_smoke -- --test-threads=1 2>&1 | tee -a "$NO_PG_LOG" >/dev/null
    NOPG2=$?
    set -e
    if [[ $NOPG1 -ne 0 || $NOPG2 -ne 0 ]]; then
        log "  no-PG integration tests FAILED: mig=$NOPG1 testkit=$NOPG2"
        TEST_EXIT=1
    fi
fi

{
    echo "IM1.0 Unit Test Summary"
    echo "  stamp      : $STAMP"
    echo "  filter     : '$FILTER'"
    echo "  build      : $([[ "$SKIP_BUILD" == "1" ]] && echo SKIPPED || echo OK)"
    echo "  build_sec  : $BUILD_ELAPSED"
    echo "  test_sec   : $TEST_ELAPSED"
    echo "  exit_code  : $TEST_EXIT"
    if grep -E '^test result: ' "$TEST_LOG" >/dev/null 2>&1; then
        TOTAL_P=0; TOTAL_F=0
        while IFS= read -r line; do
            BINARY=$(echo "$line" | sed -E 's/^test result: (\w+)\. /BINARY=/' | cut -d' ' -f1)
            STATE=$(echo "$line" | sed -E 's/^test result: (\w+)\..*/\1/')
            PASSED=$(echo "$line" | sed -E 's/.* ([0-9]+) passed.*/\1/')
            FAILED=$(echo "$line" | sed -E 's/.* ([0-9]+) failed.*/\1/')
            echo "  binary     : $BINARY  state=$STATE passed=$PASSED failed=$FAILED"
            TOTAL_P=$((TOTAL_P + PASSED))
            TOTAL_F=$((TOTAL_F + FAILED))
        done < <(grep -E '^test result: ' "$TEST_LOG")
        echo "  TOTAL      : passed=$TOTAL_P failed=$TOTAL_F"
    else
        echo "  TOTAL      : (no test result line found; check $TEST_LOG)"
    fi
} > "$SUMMARY_FILE"
log "[3/3] summary written: $SUMMARY_FILE"
sed 's/^/  /' "$SUMMARY_FILE" | while read -r line; do log "$line"; done

if [[ $TEST_EXIT -ne 0 ]]; then
    log "[run_ut] FAILED. See $TEST_LOG"
    exit 1
fi
log "[run_ut] OK"
exit $TEST_EXIT