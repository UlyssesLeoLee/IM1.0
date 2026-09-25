#!/usr/bin/env bash
# ============================================================================
# run_all_tests.sh — ST 编排 (回归测试总入口, POSIX 等价物)
#
# 目的: 编排 UT → IT → ST 三层, 形成完整回归测试 (POSIX 等价 run_all_tests.ps1)。
#
# 用法:
#   bash tests/scripts/run_all_tests.sh
#   bash tests/scripts/run_all_tests.sh --skip-it
#   bash tests/scripts/run_all_tests.sh --skip-st
#   bash tests/scripts/run_all_tests.sh --skip-build --keep-db
#
# 返回: 0 = 全部通过; 非 0 = 任一层失败
# 范围: Git Bash / WSL / macOS / Linux
# 作者: Mavis 接手 agent per DEC-008 (2026-09-20)
# ============================================================================

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LOGS_DIR="${REPO_ROOT}/tests/logs"
REPORTS_DIR="${REPO_ROOT}/tests/reports"
DATABASE_URL="${IM_TEST_DATABASE_URL:-postgres://leo19@127.0.0.1:5544/postgres}"

SKIP_UT="0"; SKIP_IT="0"; SKIP_ST="0"
SKIP_BUILD="0"; KEEP_DB="0"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --skip-ut)     SKIP_UT="1"; shift ;;
        --skip-it)     SKIP_IT="1"; shift ;;
        --skip-st)     SKIP_ST="1"; shift ;;
        --skip-build)  SKIP_BUILD="1"; shift ;;
        --keep-db)     KEEP_DB="1"; shift ;;
        --database-url) DATABASE_URL="$2"; shift 2 ;;
        -h|--help)
            grep -E '^# 用途|^# 用法' "${BASH_SOURCE[0]}" | head -10
            exit 0
            ;;
        *) echo "unknown arg: $1" >&2; exit 64 ;;
    esac
done

mkdir -p "$LOGS_DIR" "$REPORTS_DIR"
STAMP="$(date +%Y%m%d-%H%M%S)"
MASTER_LOG="${LOGS_DIR}/regression-${STAMP}.log"
REPORT_MD="${REPORTS_DIR}/regression-${STAMP}.md"
REPORT_JSON="${REPORTS_DIR}/regression-${STAMP}.json"

log() { printf '[%s] %s\n' "$(date -Iseconds)" "$*" | tee -a "$MASTER_LOG" >&2 ; }

log "=== run_all_tests (regression orchestrator) START ==="
log "  repo       = $REPO_ROOT"
log "  db         = $DATABASE_URL"
log "  skip_flags = ut=$SKIP_UT it=$SKIP_IT st=$SKIP_ST build=$SKIP_BUILD"

# results store: layer|status|duration_s|log_path
RESULTS=()
UT_STATUS=""; IT_STATUS=""; ST_STATUS=""
UT_EXIT=0;    IT_EXIT=0;    ST_EXIT=0
UT_DUR=0;     IT_DUR=0;     ST_DUR=0

# --- Layer 1: UT ---
if [[ "$SKIP_UT" != "1" ]]; then
    log ""
    log "=========================================="
    log " Layer 1 / 3 : UT  (Unit Tests)"
    log "=========================================="
    UT_START=$(date +%s)
    UT_ARGS=()
    [[ "$SKIP_BUILD" == "1" ]] && UT_ARGS+=("--skip-build")
    set +e
    bash "${REPO_ROOT}/tests/scripts/run_ut.sh" "${UT_ARGS[@]}"
    UT_EXIT=$?
    set -e
    UT_DUR=$(( $(date +%s) - UT_START ))
    UT_STATUS=$([[ $UT_EXIT -eq 0 ]] && echo PASS || echo FAIL)
    UT_LOG=$(ls -t "${LOGS_DIR}"/ut-summary-*.txt 2>/dev/null | head -1 || echo "")
    RESULTS+=("UT|${UT_STATUS}|${UT_EXIT}|${UT_DUR}|${UT_LOG}")
    [[ $UT_EXIT -ne 0 ]] && log "[UT] FAILED exit=$UT_EXIT"
else
    log "[Layer 1] UT -- SKIPPED"
fi

# --- Layer 2: IT ---
if [[ "$SKIP_IT" != "1" ]]; then
    log ""
    log "=========================================="
    log " Layer 2 / 3 : IT  (Integration Tests)"
    log "=========================================="
    IT_START=$(date +%s)
    IT_ARGS=()
    [[ "$SKIP_BUILD" == "1" ]] && IT_ARGS+=("--skip-build")
    [[ "$KEEP_DB" == "1" ]]    && IT_ARGS+=("--keep-db")
    set +e
    export IM_TEST_DATABASE_URL="$DATABASE_URL"
    bash "${REPO_ROOT}/tests/scripts/run_it.sh" "${IT_ARGS[@]}"
    IT_EXIT=$?
    set -e
    IT_DUR=$(( $(date +%s) - IT_START ))
    IT_STATUS=$([[ $IT_EXIT -eq 0 ]] && echo PASS || echo FAIL)
    IT_LOG=$(ls -t "${LOGS_DIR}"/it-summary-*.txt 2>/dev/null | head -1 || echo "")
    RESULTS+=("IT|${IT_STATUS}|${IT_EXIT}|${IT_DUR}|${IT_LOG}")
    [[ $IT_EXIT -ne 0 ]] && log "[IT] FAILED exit=$IT_EXIT"
else
    log "[Layer 2] IT -- SKIPPED"
fi

# --- Layer 3: ST ---
if [[ "$SKIP_ST" != "1" ]]; then
    log ""
    log "=========================================="
    log " Layer 3 / 3 : ST  (System Test — gateway smoke)"
    log "=========================================="
    ST_START=$(date +%s)
    ST_LOG="${LOGS_DIR}/st-${STAMP}.log"

    GW_BIN=""
    for cand in \
        "${REPO_ROOT}/target/release/im-gateway" \
        "${REPO_ROOT}/target/debug/im-gateway" \
        "/e/DevCache/cargo/target/im1.0/debug/im-gateway" \
        "/e/DevCache/cargo/target/im1.0/release/im-gateway" ; do
        if [[ -x "$cand" ]]; then GW_BIN="$cand"; fi
    done

    if [[ -z "$GW_BIN" ]]; then
        log "[ST] im-gateway binary not found."
        log "[ST] Build first:  cargo build -p im-gateway"
        log "[ST] Skipping smoke — Layer 3 INCONCLUSIVE"
        ST_STATUS="INCONCLUSIVE"
        RESULTS+=("ST|${ST_STATUS}|0|0|${ST_LOG}")
    else
        log "[ST] im-gateway binary = $GW_BIN"
        IM_GATEWAY__HTTP__BIND="0.0.0.0:18080" \
        IM_GATEWAY__WS__BIND="0.0.0.0:18081" \
        IM_GATEWAY__GRPC__BIND="0.0.0.0:19001" \
        IM_GATEWAY__DB__DSN="postgres://postgres@127.0.0.1:5555/im1dev" \
        IM_GATEWAY__AUTH__JWT_SECRET="${IM_TEST_JWT_SECRET:-test-only-do-not-use-in-prod}" \
        "$GW_BIN" >> "$ST_LOG" 2>&1 &
        GW_PID=$!
        # wait for /healthz
        ST_EXIT=1
        for i in {1..30}; do
            sleep 0.5
            if curl -fsS "http://127.0.0.1:18080/healthz" >/dev/null 2>&1; then
                log "  healthz OK"
                ST_EXIT=0
                break
            fi
            if ! kill -0 "$GW_PID" 2>/dev/null; then
                log "  im-gateway exited prematurely"
                break
            fi
        done
        # stop
        kill "$GW_PID" 2>/dev/null || true
        wait "$GW_PID" 2>/dev/null || true
        ST_STATUS=$([[ $ST_EXIT -eq 0 ]] && echo PASS || echo FAIL)
        ST_DUR=$(( $(date +%s) - ST_START ))
        RESULTS+=("ST|${ST_STATUS}|${ST_EXIT}|${ST_DUR}|${ST_LOG}")
    fi
else
    log "[Layer 3] ST -- SKIPPED"
fi

# --- summary ---
log ""
log "=========================================="
log " Regression summary"
log "=========================================="
OVERALL="PASS"
for r in "${RESULTS[@]}"; do
    IFS='|' read -r LAYER STATUS EXIT DUR LOGPATH <<< "$r"
    log "  ${LAYER}  ${STATUS}  exit=${EXIT}  duration=${DUR}s"
    [[ "$STATUS" == "FAIL" ]] && OVERALL="FAIL"
done

# write markdown
{
    echo "# IM1.0 回归测试报告"
    echo ""
    echo "- **stamp**: ${STAMP}"
    echo "- **repo**: ${REPO_ROOT}"
    echo "- **db**: ${DATABASE_URL}"
    echo "- **skip**: ut=${SKIP_UT} it=${SKIP_IT} st=${SKIP_ST} build=${SKIP_BUILD} keep_db=${KEEP_DB}"
    echo "- **master_log**: ${MASTER_LOG}"
    echo ""
    echo "## 结果汇总"
    echo ""
    echo "| Layer | Status | Exit | Duration(s) | Log |"
    echo "|---|---|---|---|---|"
    for r in "${RESULTS[@]}"; do
        IFS='|' read -r LAYER STATUS EXIT DUR LOGPATH <<< "$r"
        RELLOG="${LOGPATH#${REPO_ROOT}/}"
        echo "| ${LAYER} | ${STATUS} | ${EXIT} | ${DUR} | \`${RELLOG}\` |"
    done
    echo ""
    echo "**整体**: $([[ "$OVERALL" == "PASS" ]] && echo "✅ PASS" || echo "❌ FAIL")"
} > "$REPORT_MD"

# write json (no jq dep — build manually)
{
    echo "{"
    echo "  \"stamp\": \"${STAMP}\","
    echo "  \"repo\": \"${REPO_ROOT}\","
    echo "  \"db\": \"${DATABASE_URL}\","
    echo "  \"skip_ut\": ${SKIP_UT},"
    echo "  \"skip_it\": ${SKIP_IT},"
    echo "  \"skip_st\": ${SKIP_ST},"
    echo "  \"skip_build\": ${SKIP_BUILD},"
    echo "  \"keep_db\": ${KEEP_DB},"
    echo "  \"overall\": \"${OVERALL}\","
    echo "  \"layers\": ["
    FIRST=1
    for r in "${RESULTS[@]}"; do
        IFS='|' read -r LAYER STATUS EXIT DUR LOGPATH <<< "$r"
        [[ $FIRST -eq 1 ]] || echo ","
        FIRST=0
        printf '    {"layer":"%s","status":"%s","exit":%s,"duration_s":%s,"log":"%s"}' \
            "$LAYER" "$STATUS" "$EXIT" "$DUR" "$LOGPATH"
    done
    echo ""
    echo "  ]"
    echo "}"
} > "$REPORT_JSON"

log "  report (md)   = ${REPORT_MD}"
log "  report (json) = ${REPORT_JSON}"
log "=== run_all_tests END ==="

[[ "$OVERALL" == "FAIL" ]] && exit 1
exit 0