# ============================================================================
# run_it.ps1 — Integration Test 编排脚本 (IT 层)
#
# 目的: 跑需要 PostgreSQL 的集成测试 (Rust integration tests under
#       crates/*/tests/, 以及 im-testkit 端到端 mock server 测试)。
#       隐含先 step 1 = setup_test_db.ps1 (用户单独跑, 或 -SetupDb)。
#
# 用法:
#   pwsh tests/scripts/run_it.ps1                       # 假定 db 已起
#   pwsh tests/scripts/run_it.ps1 -SetupDb             # 先 setup 再跑
#   pwsh tests/scripts/run_it.ps1 -TestName pg_repos_integration
#   pwsh tests/scripts/run_it.ps1 -KeepDb
#   pwsh tests/scripts/run_it.ps1 -DatabaseUrl "postgres://leo19@127.0.0.1:5544/postgres"
#
# 环境变量 (推荐):
#   IM_TEST_DATABASE_URL  缺省: postgres://leo19@127.0.0.1:5544/postgres
#
# 返回: 0 = 全部通过; 1 = 有失败
# 范围: Windows PowerShell 5.1+ / PowerShell 7+
# 作者: Mavis 接手 agent per DEC-008 (2026-09-20)
# ============================================================================

[CmdletBinding()]
param(
    [string]$TestName       = "",   # 限定单个 integration test (cargo --test)
    [switch]$SetupDb,               # 先 setup_test_db.ps1
    [switch]$TeardownDb,            # 跑完 teardown_test_db.ps1
    [switch]$KeepDb,                # 不动 db (即使 -SetupDb / -TeardownDb)
    [switch]$SkipBuild,             # skip cargo build
    [string]$DatabaseUrl = $env:IM_TEST_DATABASE_URL,
    [string]$RepoRoot    = (Resolve-Path (Join-Path $PSScriptRoot ".." "..")).Path,
    [string]$LogDir      = (Join-Path $PSScriptRoot ".." "logs"),
    [string]$TargetDir   = ""      # forwarded CARGO_TARGET_DIR
)

$ErrorActionPreference = 'Stop'

# ----------------------------------------------------------------------------
# 0. Pre-flight
# ----------------------------------------------------------------------------
if (-not $DatabaseUrl) {
    $DatabaseUrl = "postgres://leo19@127.0.0.1:5544/postgres"
}

Write-Host "[run_it] repo   = $RepoRoot"
Write-Host "[run_it] db     = $DatabaseUrl"
Write-Host "[run_it] test   = '$TestName'"
Write-Host "[run_it] logs   = $LogDir"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not on PATH."
}

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logFile = Join-Path $LogDir "it-$stamp.log"
$summaryFile = Join-Path $LogDir "it-summary-$stamp.txt"

function Log($msg) {
    $line = "[$(Get-Date -Format o)] $msg"
    Write-Host $line
    Add-Content -Path $logFile -Value $line
}

# ----------------------------------------------------------------------------
# 1. (optional) setup_test_db
# ----------------------------------------------------------------------------
if ($SetupDb -and -not $KeepDb) {
    Log "[1/5] setup_test_db.ps1 (with IM_TEST_DATABASE_URL=$DatabaseUrl)"
    $env:IM_TEST_DATABASE_URL = $DatabaseUrl
    & (Join-Path $PSScriptRoot "setup_test_db.ps1") *>&1 | Tee-Object -FilePath (Join-Path $LogDir "it-setup-$stamp.log")
    if ($LASTEXITCODE -ne 0) { throw "setup_test_db failed (exit=$LASTEXITCODE)" }
} else {
    Log "[1/5] setup_test_db -- SKIPPED"
}

# ----------------------------------------------------------------------------
# 2. PG reachability pre-check
# ----------------------------------------------------------------------------
Log "[2/5] PG reachability pre-check"
$pgHost = ([System.Uri]$DatabaseUrl).Host
$pgPort = ([System.Uri]$DatabaseUrl).Port
$canConnect = Test-NetConnection -ComputerName $pgHost -Port $pgPort -InformationLevel Quiet -WarningAction SilentlyContinue
if (-not $canConnect) {
    Log ("  [X] cannot reach {0}:{1} - integration tests will fail" -f $pgHost, $pgPort)
    Log "  hint: run scripts/init-pg18-b1.sh / scripts/restart-pg18-b1-all.sh"
    $reachability = "DOWN"
} else {
    Log ("  [OK] {0}:{1} reachable" -f $pgHost, $pgPort)
    $reachability = "UP"
}

# ----------------------------------------------------------------------------
# 3. cargo build
# ----------------------------------------------------------------------------
$buildStart = Get-Date
if (-not $SkipBuild) {
    Log "[3/5] cargo build --workspace --tests"
    Push-Location $RepoRoot
    try {
        if ($TargetDir) { $env:CARGO_TARGET_DIR = $TargetDir }
        cargo build --workspace --tests 2>&1 | Tee-Object -FilePath (Join-Path $LogDir "it-build-$stamp.log")
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed (exit=$LASTEXITCODE). See $logFile."
        }
    } finally {
        Pop-Location
    }
} else {
    Log "[3/5] cargo build -- SKIPPED"
}
$buildElapsed = (Get-Date) - $buildStart
Log "[3/5] build elapsed: $($buildElapsed.ToString('hh\:mm\:ss'))"

# ----------------------------------------------------------------------------
# 4. cargo test --test <name> (integration)
# ----------------------------------------------------------------------------
$testStart = Get-Date
Log "[4/5] cargo test --workspace --tests --test-threads=1 (DATABASE_URL=$DatabaseUrl)"
$testLog = Join-Path $LogDir "it-test-$stamp.log"

$env:DATABASE_URL = $DatabaseUrl
Push-Location $RepoRoot
try {
    if ($TargetDir) { $env:CARGO_TARGET_DIR = $TargetDir }
    if ($TestName) {
        $cargoArgs = @("test", "--workspace", "--test", $TestName, "--", "--test-threads=1")
    } else {
        $cargoArgs = @("test", "--workspace", "--tests", "--no-fail-fast", "--", "--test-threads=1")
    }
    & cargo @cargoArgs 2>&1 | Tee-Object -FilePath $testLog
    $testExit = $LASTEXITCODE
} finally {
    Pop-Location
}
$testElapsed = (Get-Date) - $testStart
Log "[4/5] test  elapsed: $($testElapsed.ToString('hh\:mm\:ss'))"
Log "[4/5] test  exit  : $testExit"

# ----------------------------------------------------------------------------
# 5. (optional) teardown
# ----------------------------------------------------------------------------
if ($TeardownDb -and -not $KeepDb) {
    Log "[5/5] teardown_test_db.ps1 -Force"
    & (Join-Path $PSScriptRoot "teardown_test_db.ps1") -Force *>&1 | Tee-Object -FilePath (Join-Path $LogDir "it-teardown-$stamp.log")
} else {
    Log "[5/5] teardown -- SKIPPED"
}

# ----------------------------------------------------------------------------
# Summary
# ----------------------------------------------------------------------------
$summaryLines = @()
$summaryLines += "IM1.0 Integration Test Summary"
$summaryLines += "  stamp      : $stamp"
$summaryLines += "  pg         : $DatabaseUrl ($reachability)"
$summaryLines += "  test_name  : '$TestName'"
$summaryLines += "  build_sec  : $($buildElapsed.TotalSeconds.ToString('0.0'))"
$summaryLines += "  test_sec   : $($testElapsed.TotalSeconds.ToString('0.0'))"
$summaryLines += "  exit_code  : $testExit"

$matches = Select-String -Path $testLog -Pattern "^test result: (\w+)\. (\d+) passed; (\d+) failed" -ErrorAction SilentlyContinue
if ($matches) {
    $totalPassed = 0; $totalFailed = 0
    foreach ($m in $matches) {
        $summaryLines += ("  binary     : state=" + $m.Matches[0].Groups[1].Value + " passed=" + $m.Matches[0].Groups[2].Value + " failed=" + $m.Matches[0].Groups[3].Value)
        $totalPassed += [int]$m.Matches[0].Groups[2].Value
        $totalFailed += [int]$m.Matches[0].Groups[3].Value
    }
    $summaryLines += "  TOTAL      : passed=$totalPassed failed=$totalFailed"
} else {
    $summaryLines += "  TOTAL      : (no test result line found; check $testLog)"
}
$summaryLines | Out-File -FilePath $summaryFile -Encoding UTF8
Get-Content $summaryFile | ForEach-Object { Log "  $_" }

if ($testExit -ne 0) {
    Log "[run_it] FAILED. See $testLog"
    exit 1
}
Log "[run_it] OK"
exit $testExit