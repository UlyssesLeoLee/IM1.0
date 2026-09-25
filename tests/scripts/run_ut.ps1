# ============================================================================
# run_ut.ps1 — Unit Test 编排脚本 (UT 层)
#
# 目的: 对 IM1.0 mock 项目跑单元测试 (`cargo test --workspace --lib`),
#       不需要真实 PostgreSQL / im-gateway, 任何时候都可跑。
#       产出: tests/logs/ut-<stamp>.log + tests/logs/ut-<stamp>.junit.xml (可选)
#
# 用法:
#   pwsh tests/scripts/run_ut.ps1
#   pwsh tests/scripts/run_ut.ps1 -Filter "im_testkit"
#   pwsh tests/scripts/run_ut.ps1 -SkipBuild
#
# 返回: 0 = 全部通过; 1 = 有失败
# 范围: Windows PowerShell 5.1+ / PowerShell 7+
# 作者: Mavis 接手 agent per DEC-008 (2026-09-20)
# ============================================================================

[CmdletBinding()]
param(
    [string]$Filter = "",              # cargo test filter substring (use -Filter explicitly)
    [switch]$SkipBuild,                # skip `cargo build` step
    [switch]$NoFailFast,               # default true; pass --no-fail-fast to cargo
    [switch]$IncludeNoPgIntegration,   # also run non-PG integration tests (migration_smoke, im_testkit_smoke)
    [string]$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot ".." "..")).Path,
    [string]$TargetDir = "",
    [string]$LogDir   = (Join-Path $PSScriptRoot ".." "logs")
)

$ErrorActionPreference = 'Stop'

# ----------------------------------------------------------------------------
# 0. Pre-flight
# ----------------------------------------------------------------------------
Write-Host "[run_ut] repo    = $RepoRoot"
Write-Host "[run_ut] filter  = '$Filter'"
Write-Host "[run_ut] logs    = $LogDir"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not on PATH. Install rustup / cargo first."
}

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile  = Join-Path $LogDir "ut-$stamp.log"
$summaryFile = Join-Path $LogDir "ut-summary-$stamp.txt"

function Log($msg) {
    $line = "[$(Get-Date -Format o)] $msg"
    Write-Host $line
    Add-Content -Path $logFile -Value $line
}

# ----------------------------------------------------------------------------
# 1. Workspace build (optional)
# ----------------------------------------------------------------------------
$buildStart = Get-Date
if (-not $SkipBuild) {
    Log "[1/3] cargo build --workspace --tests (no PG required)"
    Push-Location $RepoRoot
    try {
        if ($TargetDir) {
            $env:CARGO_TARGET_DIR = $TargetDir
        }
        $buildLog = Join-Path $LogDir "ut-build-$stamp.log"
        cargo build --workspace --tests 2>&1 | Tee-Object -FilePath $buildLog
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed (exit=$LASTEXITCODE). See $logFile."
        }
    } finally {
        Pop-Location
    }
} else {
    Log "[1/3] cargo build -- SKIPPED (SkipBuild)"
}
$buildElapsed = (Get-Date) - $buildStart
Log "[1/3] build elapsed: $($buildElapsed.ToString('hh\:mm\:ss'))"

# ----------------------------------------------------------------------------
# 2. cargo test --workspace --lib (unit tests)
# ----------------------------------------------------------------------------
Log "[2/3] cargo test --workspace --lib (unit tests)"
$testStart = Get-Date
$testLog = Join-Path $LogDir "ut-test-$stamp.log"

Push-Location $RepoRoot
try {
    if ($TargetDir) {
        $env:CARGO_TARGET_DIR = $TargetDir
    }
    $cargoArgs = @("test", "--workspace", "--lib")
    if (-not $NoFailFast) {
        $cargoArgs += "--no-fail-fast"
    }
    if ($Filter) {
        $cargoArgs += "--"
        $cargoArgs += $Filter
    }

    & cargo @cargoArgs 2>&1 | Tee-Object -FilePath $testLog
    $testExit = $LASTEXITCODE
} finally {
    Pop-Location
}

# 2b. (optional) non-PG integration tests
if ($IncludeNoPgIntegration) {
    Log "[2b/3] non-PG integration tests: migration_smoke + im_testkit_smoke"
    $noPgLog = Join-Path $LogDir "ut-nopg-$stamp.log"
    Push-Location $RepoRoot
    try {
        if ($TargetDir) { $env:CARGO_TARGET_DIR = $TargetDir }
        & cargo test -p im-gateway --test migration_smoke -- --test-threads=1 2>&1 | Tee-Object -FilePath $noPgLog -Append
        $nopgExit1 = $LASTEXITCODE
        & cargo test -p im-testkit --test im_testkit_smoke -- --test-threads=1 2>&1 | Tee-Object -FilePath $noPgLog -Append
        $nopgExit2 = $LASTEXITCODE
        if ($nopgExit1 -ne 0 -or $nopgExit2 -ne 0) {
            Log "  no-PG integration tests FAILED: mig=$nopgExit1 testkit=$nopgExit2"
            $testExit = 1
        }
    } finally {
        Pop-Location
    }
}
$testElapsed = (Get-Date) - $testStart
Log "[2/3] test  elapsed: $($testElapsed.ToString('hh\:mm\:ss'))"
Log "[2/3] test  exit  : $testExit"

# ----------------------------------------------------------------------------
# 3. Parse summary (best-effort) — combines lib + non-PG integration logs
# ----------------------------------------------------------------------------
$summaryLines = @()
$summaryLines += "IM1.0 Unit Test Summary"
$summaryLines += "  stamp      : $stamp"
$summaryLines += "  filter     : '$Filter'"
$summaryLines += "  build      : $(if ($SkipBuild) { 'SKIPPED' } else { 'OK' })"
$summaryLines += "  build_sec  : $($buildElapsed.TotalSeconds.ToString('0.0'))"
$summaryLines += "  test_sec   : $($testElapsed.TotalSeconds.ToString('0.0'))"
$summaryLines += "  exit_code  : $testExit"

# Aggregate from lib test log + (optional) no-PG integration log
$totalPassed = 0; $totalFailed = 0
$logFiles = @($testLog)
if ($IncludeNoPgIntegration -and (Test-Path (Join-Path $LogDir "ut-nopg-$stamp.log"))) {
    $logFiles += (Join-Path $LogDir "ut-nopg-$stamp.log")
}
foreach ($lf in $logFiles) {
    $matches = Select-String -Path $lf -Pattern "^test result: (\w+)\. (\d+) passed; (\d+) failed" -ErrorAction SilentlyContinue
    foreach ($m in $matches) {
        $summaryLines += ("  binary     : state=" + $m.Matches[0].Groups[1].Value + " passed=" + $m.Matches[0].Groups[2].Value + " failed=" + $m.Matches[0].Groups[3].Value)
        $totalPassed += [int]$m.Matches[0].Groups[2].Value
        $totalFailed += [int]$m.Matches[0].Groups[3].Value
    }
}
$summaryLines += "  TOTAL      : passed=$totalPassed failed=$totalFailed"
$summaryLines | Out-File -FilePath $summaryFile -Encoding UTF8
Log "[3/3] summary written: $summaryFile"

Get-Content $summaryFile | ForEach-Object { Log "  $_" }

# ----------------------------------------------------------------------------
# 4. Exit
# ----------------------------------------------------------------------------
if ($testExit -ne 0) {
    Log "[run_ut] FAILED. See $testLog"
    exit 1
}
Log "[run_ut] OK"
exit $testExit