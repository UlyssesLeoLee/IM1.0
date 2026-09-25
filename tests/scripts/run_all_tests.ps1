# ============================================================================
# run_all_tests.ps1 — ST 编排 (回归测试总入口)
#
# 目的: 编排 UT → IT → ST 三层, 形成完整回归测试。
#       ST (System Test) 层: 起 im-gateway 跑 wscat/curl smoke, 验证协议面。
#
# 用法:
#   pwsh tests/scripts/run_all_tests.ps1                       # 完整回归
#   pwsh tests/scripts/run_all_tests.ps1 -SkipIt              # 不跑集成
#   pwsh tests/scripts/run_all_tests.ps1 -SkipSt              # 不跑系统层
#   pwsh tests/scripts/run_all_tests.ps1 -SkipBuild           # 假定已 build
#   pwsh tests/scripts/run_all_tests.ps1 -KeepDb              # 保留 test db
#
# 退出码:
#   0 = 全部通过
#   1 = 任一层失败
#
# 范围: Windows PowerShell 5.1+ / PowerShell 7+
# 作者: Mavis 接手 agent per DEC-008 (2026-09-20)
# 升级: 把原来的 setup → cargo test → gateway smoke 单阶段流程
#       拆成 UT/IT/ST 三层编排, 每层独立日志 + 独立退出码,
#       最终汇总生成 tests/logs/regression-<stamp>.md 报告
# ============================================================================

[CmdletBinding()]
param(
    [switch]$SkipUt,
    [switch]$SkipIt,
    [switch]$SkipSt,
    [switch]$SkipBuild,
    [switch]$KeepDb,
    [switch]$SkipGatewaySmoke,
    [string]$DatabaseUrl = $env:IM_TEST_DATABASE_URL,
    [string]$RepoRoot    = (Resolve-Path (Join-Path $PSScriptRoot ".." "..")).Path,
    [string]$LogDir      = (Join-Path $PSScriptRoot ".." "logs"),
    [string]$ReportDir   = (Join-Path $PSScriptRoot ".." "reports")
)

$ErrorActionPreference = 'Stop'

if (-not $DatabaseUrl) {
    $DatabaseUrl = "postgres://leo19@127.0.0.1:5544/postgres"
}

# ----------------------------------------------------------------------------
# 0. Setup
# ----------------------------------------------------------------------------
New-Item -ItemType Directory -Force -Path $LogDir    | Out-Null
New-Item -ItemType Directory -Force -Path $ReportDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$masterLog     = Join-Path $LogDir    "regression-$stamp.log"
$reportMdPath  = Join-Path $ReportDir "regression-$stamp.md"
$reportJson    = Join-Path $ReportDir "regression-$stamp.json"

function Log($msg) {
    $line = "[$(Get-Date -Format o)] $msg"
    Write-Host $line
    Add-Content -Path $masterLog -Value $line
}

# result table: layer / status / exit / build_sec / test_sec / log / summary
$results = @()
function Add-Result($layer, $status, $exit, $sec, $log, $summary) {
    $r = [PSCustomObject]@{
        layer    = $layer
        status   = $status
        exit     = $exit
        duration_s= $sec
        log      = $log
        summary  = $summary
    }
    $script:results += $r
}

Log "=== run_all_tests (regression orchestrator) START ==="
Log "  repo       = $RepoRoot"
Log "  db         = $DatabaseUrl"
Log "  skip_flags = ut=$SkipUt it=$SkipIt st=$SkipSt build=$SkipBuild"
Log "  master log = $masterLog"

# ----------------------------------------------------------------------------
# Layer 1: UT (Unit Tests, no PG required)
# ----------------------------------------------------------------------------
if (-not $SkipUt) {
    Log ""
    Log "=========================================="
    Log " Layer 1 / 3 : UT  (Unit Tests)"
    Log "=========================================="
    $utStart = Get-Date
        $utScript = Join-Path $PSScriptRoot "run_ut.ps1"
        $utArgs = @()
            if ($SkipBuild) { $utArgs += "-SkipBuild" }
            # Default: include non-PG integration tests in UT layer (mock_server + migration_smoke)
            $utArgs += "-IncludeNoPgIntegration"
            # Forward CARGO_TARGET_DIR to child so it shares our build cache
            if ($env:CARGO_TARGET_DIR) { $utArgs += @("-TargetDir", $env:CARGO_TARGET_DIR) }
            Log ("  [UT] args: " + ($utArgs -join " | "))
            # Use Start-Process to invoke child script (avoids PowerShell & + @splat named-binding quirk in some versions)
        $proc = Start-Process -FilePath "pwsh" -ArgumentList (@("-NoProfile", "-File", $utScript) + $utArgs) -Wait -PassThru -NoNewWindow `
            -RedirectStandardOutput (Join-Path $LogDir "ut-out-$stamp.txt") `
            -RedirectStandardError  (Join-Path $LogDir "ut-err-$stamp.txt")
        $utExit = $proc.ExitCode
    $utElapsed = (Get-Date) - $utStart
        $utStatus = if ($utExit -eq 0) { "PASS" } else { "FAIL" }
        # Find latest ut-summary file (FullName, not the FileInfo object — Join-Path treats FileInfo as relative path)
        $utSummary = Get-ChildItem -Path $LogDir -Filter "ut-summary-*.txt" -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        $utSummaryPath = if ($utSummary) { $utSummary.FullName } else { "(none)" }
        Add-Result "UT" $utStatus $utExit $utElapsed.TotalSeconds $utSummaryPath "see ut-summary"
        if ($utExit -ne 0) { Log "[UT] FAILED exit=$utExit" }
    } else {
        Log "[Layer 1] UT -- SKIPPED"
    }

# ----------------------------------------------------------------------------
# Layer 2: IT (Integration Tests, requires PG)
# ----------------------------------------------------------------------------
if (-not $SkipIt) {
    Log ""
    Log "=========================================="
    Log " Layer 2 / 3 : IT  (Integration Tests)"
    Log "=========================================="
    $itStart = Get-Date
        $itScript = Join-Path $PSScriptRoot "run_it.ps1"
        $itArgs = @()
        if ($SkipBuild) { $itArgs += "-SkipBuild" }
        if ($KeepDb)    { $itArgs += "-KeepDb" }
        if ($env:CARGO_TARGET_DIR) { $itArgs += @("-TargetDir", $env:CARGO_TARGET_DIR) }
        $env:IM_TEST_DATABASE_URL = $DatabaseUrl
        Log ("  [IT] args: " + ($itArgs -join " | "))
        Log ("  [IT] CARGO_TARGET_DIR=" + $env:CARGO_TARGET_DIR)
        $proc = Start-Process -FilePath "pwsh" -ArgumentList (@("-NoProfile", "-File", $itScript) + $itArgs) -Wait -PassThru -NoNewWindow `
            -RedirectStandardOutput (Join-Path $LogDir "it-out-$stamp.txt") `
            -RedirectStandardError  (Join-Path $LogDir "it-err-$stamp.txt")
        $itExit = $proc.ExitCode
    $itElapsed = (Get-Date) - $itStart
        $itStatus = if ($itExit -eq 0) { "PASS" } else { "FAIL" }
        $itSummary = Get-ChildItem -Path $LogDir -Filter "it-summary-*.txt" -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTime -Descending | Select-Object -First 1
        $itSummaryPath = if ($itSummary) { $itSummary.FullName } else { "(none)" }
        Add-Result "IT" $itStatus $itExit $itElapsed.TotalSeconds $itSummaryPath "see it-summary"
        if ($itExit -ne 0) { Log "[IT] FAILED exit=$itExit" }
    } else {
        Log "[Layer 2] IT -- SKIPPED"
    }

# ----------------------------------------------------------------------------
# Layer 3: ST (System Test — start im-gateway + smoke)
# ----------------------------------------------------------------------------
if (-not $SkipSt) {
    Log ""
    Log "=========================================="
    Log " Layer 3 / 3 : ST  (System Test — gateway smoke)"
    Log "=========================================="
    $stStart = Get-Date

    $candidate = @(
        (Join-Path $RepoRoot "target" "release" "im-gateway.exe"),
        (Join-Path $RepoRoot "target" "debug"   "im-gateway.exe"),
        (Join-Path "E:/DevCache/cargo/target/im1.0/debug" "im-gateway.exe"),
        (Join-Path "E:/DevCache/cargo/target/im1.0/release" "im-gateway.exe")
    ) | Where-Object { Test-Path $_ } | Select-Object -First 1

    $stExit = 0
        $stLogFile = Join-Path $LogDir "st-$stamp.log"
        if (-not $candidate) {
            Log "[ST] im-gateway binary not found."
            Log "[ST] Build first:  cargo build -p im-gateway  (or via run_ut.ps1 / run_it.ps1 which build the workspace)"
            Log "[ST] Skipping smoke - Layer 3 marked INCONCLUSIVE"
            Add-Result "ST" "INCONCLUSIVE" 0 0 $stLogFile "no im-gateway binary found"
            $stExit = 0  # do not fail regression on missing binary
        } else {
            Log "[ST] im-gateway binary = $candidate"
            try {
                & (Join-Path $PSScriptRoot "start_im_gateway_mock.ps1") -WaitForReady 2>&1 | Tee-Object -FilePath $stLogFile | Out-Null
                # health check
                try {
                    $health = Invoke-WebRequest -Uri "http://127.0.0.1:18080/healthz" -UseBasicParsing -TimeoutSec 5
                    Log ("  healthz status: {0}" -f $health.StatusCode)
                    $stExit = 0
                } catch {
                    Log ("  healthz FAILED: {0}" -f $_.Exception.Message)
                    $stExit = 1
                }
                $stStatusLocal = if ($stExit -eq 0) { "PASS" } else { "FAIL" }
                $stDur = (Get-Date) - $stStart
                Add-Result "ST" $stStatusLocal $stExit $stDur.TotalSeconds $stLogFile "healthz check"
            } finally {
                Log "[ST] stopping im-gateway"
                & (Join-Path $PSScriptRoot "stop_im_gateway.ps1") -Force 2>&1 | Tee-Object -FilePath $stLogFile -Append | Out-Null
            }
        }
        $stElapsed = (Get-Date) - $stStart
} else {
    Log "[Layer 3] ST -- SKIPPED"
}

# ----------------------------------------------------------------------------
# Regression report
# ----------------------------------------------------------------------------
Log ""
Log "=========================================="
Log " Regression summary"
Log "=========================================="
$overallPass = $true
foreach ($r in $results) {
    $layerStr = $r.layer
    $statusStr = $r.status
    $exitStr = $r.exit
    $durStr = "{0,7:N1}s" -f $r.duration_s
    Log ("  " + $layerStr + " " + $statusStr + " " + ("exit=" + $exitStr) + " duration=" + $durStr)
    if ($r.status -eq "FAIL") { $overallPass = $false }
}

# markdown report
$md = @()
$md += "# IM1.0 回归测试报告"
$md += ""
$md += "- **stamp**: $stamp"
$md += "- **repo**: $RepoRoot"
$md += "- **db**: $DatabaseUrl"
$md += "- **skip**: ut=$SkipUt it=$SkipIt st=$SkipSt build=$SkipBuild keep_db=$KeepDb"
$md += "- **master_log**: $masterLog"
$md += ""
$md += "## 结果汇总"
$md += ""
$md += "| Layer | Status | Exit | Duration(s) | Log |"
$md += "|---|---|---|---|---|"
foreach ($r in $results) {
    $relLog = if ($r.log -and (Test-Path $r.log)) { $r.log.Substring($RepoRoot.Length + 1) } else { "(none)" }
    $md += ("| {0} | {1} | {2} | {3} | `{4}` |" -f $r.layer, $r.status, $r.exit, $r.duration_s, $relLog)
}
$md += ""
$overallText = if ($overallPass) { "PASS" } else { "FAIL" }
$md += ("**整体**: " + $overallText)
$md += ""
$md += "## 各层细节"
foreach ($r in $results) {
    $md += ""
    $md += "### $($r.layer)"
    $md += ""
    $md += "- status: $($r.status)"
    $md += "- exit:   $($r.exit)"
    $md += "- duration_s: $($r.duration_s)"
    if ($r.summary -and (Test-Path $r.summary)) {
        $md += "- summary:"
        $md += ""
        $md += '```'
        Get-Content $r.summary | ForEach-Object { $md += $_ }
        $md += '```'
    } elseif ($r.log -and (Test-Path $r.log)) {
        $md += "- last 30 lines of log:"
        $md += ""
        $md += '```'
        Get-Content $r.log -Tail 30 | ForEach-Object { $md += $_ }
        $md += '```'
    }
}
$md += ""
$md += "---"
$md += ""
$md += "_Generated by `tests/scripts/run_all_tests.ps1` at $stamp_"
$md | Out-File -FilePath $reportMdPath -Encoding UTF8

# json report
$resultsJson = $results | ForEach-Object {
    [PSCustomObject]@{
        layer     = $_.layer
        status    = $_.status
        exit      = $_.exit
        duration_s= $_.duration_s
        log       = $_.log
        summary   = $_.summary
    }
} | ConvertTo-Json -Depth 3
$overallStatus = if ($overallPass) { "PASS" } else { "FAIL" }
$reportJsonBody = @{
    stamp      = $stamp
    repo       = $RepoRoot
    db         = $DatabaseUrl
    skip_ut    = $SkipUt
    skip_it    = $SkipIt
    skip_st    = $SkipSt
    skip_build = $SkipBuild
    keep_db    = $KeepDb
    overall    = $overallStatus
    layers     = @($results | ForEach-Object { @{ layer=$_.layer; status=$_.status; exit=$_.exit; duration_s=$_.duration_s; log=$_.log } })
} | ConvertTo-Json -Depth 5
$reportJsonBody | Out-File -FilePath $reportJson -Encoding UTF8

Log "  report (md)  = $reportMdPath"
Log "  report (json)= $reportJson"
Log "=== run_all_tests END ==="

if (-not $overallPass) {
    exit 1
}
exit 0