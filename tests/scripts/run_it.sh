#!/usr/bin/env bash
# ============================================================================
# run_it.sh — Integration Test 编排脚本 (IT 层, POSIX 等价物)
#
# 目的: 跑需要 PostgreSQL 的集成测试。
#
# 用法:
#   bash tests/scripts/run_it.sh
#   bash tests/scripts/run_it.sh --setup-db
#   bash tests/scripts/run_it.sh --test pg_repos_integration
#   bash tests/scripts/run_it.sh --database-url "postgres://leo19@127.0.0.1:5544/postgres"
#   bash tests/scripts/run_it.sh --teardown-db
#
# 返回: 0 = 全部通过; 非 0 = 有失败
# 范围: Git Bash / WSL / macOS / Linux
# 作者: Mavis 接手 agent per DEC-008 (2026-09-20)
# ============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOGS_DIR="${REPO_ROOT}/tests/logs"
DATABASE_URL="${IM_TEST_DATABASE_URL:-postgres://leo19@127.0.0.1:5544/postgres}"
TEST_NAME=""
SETUP_DB="0"
TEARDOWN_DB="0"
KEEP_DB="0"
SKIP_BUILD="0"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --setup-db)     SETUP_DB="1"; shift ;;
        --teardown-db)  TEARDOWN_DB="1"; shift ;;
        --keep-db)      KEEP_DB="1"; shift ;;
        --skip-build)   SKIP_BUILD="1"; shift ;;
        --test)         TEST_NAME="$2"; shift 2 ;;
        --database-url) DATABASE_URL="$2"; shift 2 ;;
        -h|--help)
            grep -E '^# 用途|^# 用法' "${BASH_SOURCE[0]}" | head -10
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 64 ;;
    esac
done

mkdir -p "$LOGS_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
LOG_FILE="${LOGS_DIR}/it-${STAMP}.log"
SUMMARY_FILE="${LOGS_DIR}/it-summary-${STAMP}.txt"

log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$LOG_FILE" >&2 ; }

log "[run_it] repo = $REPO_ROOT"
log "[run_it] db   = $DATABASE_URL"
log "[run_it] test = '$TEST_NAME'"
log "[run_it] logs = $LOGS_DIR"

command -v cargo >/dev/null || { echo "cargo not on PATH" >&2; exit 127; }

if [[ "$SETUP_DB" == "1" && "$KEEP_DB" != "1" ]]; then
    log "[1/5] setup_test_db.ps1 (with IM_TEST_DATABASE_URL=$DATABASE_URL)"
    export IM_TEST_DATABASE_URL="$DATABASE_URL"
    if ! bash "${REPO_ROOT}/tests/scripts/setup_test_db.ps1" 2>&1 | tee "${LOGS_DIR}/it-setup-${STAMP}.log" >&2; then
        log "[1/5] setup_test_db FAILED"
        exit 1
    fi
else
    log "[1/5] setup_test_db -- SKIPPED"
fi

# PG reachability
PG_HOST=$(echo "$DATABASE_URL" | sed -E 's|.*://[^@]+@([^:/]+).*|\1|')
PG_PORT=$(echo "$DATABASE_URL" | sed -E 's|.*://[^@]+@[^:]+:([0-9]+).*|\1|')
log "[2/5] PG reachability pre-check ($PG_HOST:$PG_PORT)"
REACH="UP"
if command -v pg_isready >/dev/null 2>&1; then
    if ! pg_isready -h "$PG_HOST" -p "$PG_PORT" -q; then
        REACH="DOWN"
        log "  ✗ PG unreachable — integration tests will fail"
        log "  hint: run scripts/init-pg18-b1.sh / scripts/restart-pg18-b1-all.sh"
    else
        log "  ✓ PG reachable"
    fi
else
    log "  (pg_isready not on PATH; skipping reachability check)"
fi

BUILD_START=$(date +%s)
if [[ "$SKIP_BUILD" == "1" ]]; then
    log "[3/5] cargo build -- SKIPPED"
else
    log "[3/5] cargo build --workspace --tests"
    if ! cargo build --workspace --tests 2>&1 | tee "${LOGS_DIR}/it-build-${STAMP}.log" >&2; then
        log "[3/5] cargo build FAILED"
        exit 1
    fi
fi
BUILD_ELAPSED=$(( $(date +%s) - BUILD_START ))
log "[3/5] build elapsed: ${BUILD_ELAPSED}s"

TEST_START=$(date +%s)
log "[4/5] cargo test --workspace --tests --test-threads=1 (DATABASE_URL=$DATABASE_URL)"
TEST_LOG="${LOGS_DIR}/it-test-${STAMP}.log"
set +e
export DATABASE_URL="$DATABASE_URL"
if [[ -n "$TEST_NAME" ]]; then
    cargo test --workspace --test "$TEST_NAME" -- --test-threads=1 2>&1 | tee "$TEST_LOG" >&2
else
    cargo test --workspace --tests --no-fail-fast -- --test-threads=1 2>&1 | tee "$TEST_LOG" >&2
fi
TEST_EXIT=$?
set -e
TEST_ELAPSED=$(( $(date +%s) - TEST_START ))
log "[4/5] test  elapsed: ${TEST_ELAPSED}s"
log "[4/5] test  exit  : $TEST_EXIT"

if [[ "$TEARDOWN_DB" == "1" && "$KEEP_DB" != "1" ]]; then
    log "[5/5] teardown_test_db.ps1 -Force"
    bash "${REPO_ROOT}/tests/scripts/teardown_test_db.ps1" -Force 2>&1 | tee "${LOGS_DIR}/it-teardown-${STAMP}.log" >&2 || true
else
    log "[5/5] teardown -- SKIPPED"
fi

{
    echo "IM1.0 Integration Test Summary"
    echo "  stamp      : $STAMP"
    echo "  pg         : $DATABASE_URL ($REACH)"
    echo "  test_name  : '$TEST_NAME'"
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
log "[5/5] summary written: $SUMMARY_FILE"
sed 's/^/  /' "$SUMMARY_FILE" | while read -r line; do log "$line"; done

if [[ $TEST_EXIT -ne 0 ]]; then
    log "[run_it] FAILED. See $TEST_LOG"
    exit 1
fi
log "[run_it] OK"
exit $TEST_EXIT