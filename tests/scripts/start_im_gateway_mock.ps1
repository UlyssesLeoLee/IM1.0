# ============================================================================
# start_im_gateway_mock.ps1
# Purpose: Start the local im-gateway binary against the per-developer test
#          DB. Used for end-to-end WS / REST / gRPC exercising.
# Scope:   Windows PowerShell 5.1+ / PowerShell 7+
# Does NOT: build the binary, install software, or modify the test DB.
# Author:  Mavis 接手 agent per DEC-008 (2026-08-31)
# ============================================================================

[CmdletBinding()]
param(
    [string]$DbName = "im1test_$($env:USERNAME -replace '[^A-Za-z0-9]','_')",
    [string]$DbUser = "postgres",
    [string]$DbHost = "127.0.0.1",
    [int]   $DbPort = 5555,
    [string]$RepoRoot       = (Resolve-Path (Join-Path $PSScriptRoot ".." "..")).Path,
    [int]   $HttpPort       = 18080,
    [int]   $WsPort         = 18081,
    [int]   $GrpcPort       = 19001,
    [switch]$WaitForReady
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# 1. Locate the im-gateway binary
# ---------------------------------------------------------------------------
$candidate = @(
    (Join-Path $RepoRoot "target" "release" "im-gateway.exe"),
    (Join-Path $RepoRoot "target" "debug"   "im-gateway.exe")
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if (-not $candidate) {
    Write-Warning "im-gateway binary not built yet."
    Write-Warning "Build it first:  cargo build -p im-gateway"
    Write-Warning "This script will START ONCE the binary exists; nothing to do now."
    return
}

Write-Host "[start_im_gateway_mock] using binary: $candidate"

# ---------------------------------------------------------------------------
# 2. Compose environment
# ---------------------------------------------------------------------------
$env:IM_GATEWAY__HTTP__BIND      = "0.0.0.0:$HttpPort"
$env:IM_GATEWAY__WS__BIND        = "0.0.0.0:$WsPort"
$env:IM_GATEWAY__GRPC__BIND      = "0.0.0.0:$GrpcPort"
$env:IM_GATEWAY__DB__DSN         = "postgres://$DbUser@${DbHost}:${DbPort}/$DbName"
$env:IM_GATEWAY__RUST_LOG        = "info,im_gateway=debug,sqlx=warn"
$env:IM_GATEWAY__AUTH__JWT_SECRET = if ($env:IM_TEST_JWT_SECRET) { $env:IM_TEST_JWT_SECRET } else { "test-only-do-not-use-in-prod" }

Write-Host "[start_im_gateway_mock] http :$HttpPort  ws :$WsPort  grpc :$GrpcPort"
Write-Host "[start_im_gateway_mock] db   $DbName on ${DbHost}:${DbPort}"
Write-Host "[start_im_gateway_mock] press Ctrl-C to stop"

# ---------------------------------------------------------------------------
# 3. Foreground launch (caller can pipe to Tee-Object for logging)
# ---------------------------------------------------------------------------
if ($WaitForReady) {
    Write-Host "[start_im_gateway_mock] starting (wait-for-ready)..."
    $proc = Start-Process -FilePath $candidate -PassThru -NoNewWindow
    $readyDeadline = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $readyDeadline) {
        if ($proc.HasExited) { throw "im-gateway exited prematurely (code=$($proc.ExitCode))" }
        if (Test-NetConnection -ComputerName 127.0.0.1 -Port $HttpPort -InformationLevel Quiet -WarningAction SilentlyContinue) {
            Write-Host "[start_im_gateway_mock] http port $HttpPort is open"
            break
        }
        Start-Sleep -Milliseconds 500
    }
    Write-Host "[start_im_gateway_mock] ready. pid=$($proc.Id)"
    return $proc
} else {
    & $candidate
}
