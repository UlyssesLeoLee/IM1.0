# ============================================================================
# run_all_tests.ps1
# Purpose: Orchestrate the full local test loop:
#            1) setup test db
#            2) (optional) cargo test -p im-core
#            3) (optional) start im-gateway + WS/REST smoke
#            4) teardown test db
# Scope:   Windows PowerShell 5.1+ / PowerShell 7+
# Author:  Mavis 接手 agent per DEC-008 (2026-08-31)
# ============================================================================

[CmdletBinding()]
param(
    [switch]$SkipCargoTest,
    [switch]$SkipGatewaySmoke,
    [switch]$KeepDb
)

$ErrorActionPreference = 'Stop'
$here    = $PSScriptRoot
$repo    = (Resolve-Path (Join-Path $here ".." "..")).Path
$logsDir = Join-Path $here "logs"
New-Item -ItemType Directory -Force -Path $logsDir | Out-Null

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$log   = Join-Path $logsDir "run_all_tests-$stamp.log"

function Log($msg) {
    $line = "[$(Get-Date -Format o)] $msg"
    Write-Host $line
    Add-Content -Path $log -Value $line
}

Log "=== run_all_tests START ==="
Log "repo       = $repo"
Log "log file   = $log"

# ---- 1. setup test db --------------------------------------------------------
Log "[1/4] setup_test_db.ps1"
& (Join-Path $here "setup_test_db.ps1") *>&1 | Tee-Object -FilePath (Join-Path $logsDir "setup-$stamp.log")
if ($LASTEXITCODE -ne 0) { throw "setup_test_db failed (exit=$LASTEXITCODE)" }

# ---- 2. cargo test -p im-core ----------------------------------------------
if (-not $SkipCargoTest) {
    Log "[2/4] cargo test -p im-core"
    Push-Location $repo
    try {
        cargo test -p im-core 2>&1 | Tee-Object -FilePath (Join-Path $logsDir "cargo-test-$stamp.log")
        if ($LASTEXITCODE -ne 0) { throw "cargo test -p im-core failed (exit=$LASTEXITCODE)" }
    } finally {
        Pop-Location
    }
} else {
    Log "[2/4] cargo test -p im-core — SKIPPED"
}

# ---- 3. gateway smoke -------------------------------------------------------
if (-not $SkipGatewaySmoke) {
    Log "[3/4] start_im_gateway_mock.ps1 (background) + curl smoke"
    $gw = & (Join-Path $here "start_im_gateway_mock.ps1") -WaitForReady
    try {
        $health = Invoke-WebRequest -Uri "http://127.0.0.1:18080/healthz" -UseBasicParsing -TimeoutSec 5
        Log "  healthz status: $($health.StatusCode)"
    } catch {
        Log "  healthz failed: $_"
    } finally {
        Log "  stopping im-gateway"
        & (Join-Path $here "stop_im_gateway.ps1") -Force
    }
} else {
    Log "[3/4] gateway smoke — SKIPPED"
}

# ---- 4. teardown -------------------------------------------------------------
if (-not $KeepDb) {
    Log "[4/4] teardown_test_db.ps1"
    & (Join-Path $here "teardown_test_db.ps1") -Force *>&1 | Tee-Object -FilePath (Join-Path $logsDir "teardown-$stamp.log")
} else {
    Log "[4/4] teardown — SKIPPED (KeepDb)"
}

Log "=== run_all_tests END ==="
Write-Host ""
Write-Host "Full log: $log"
