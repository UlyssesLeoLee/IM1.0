# ============================================================================
# stop_im_gateway.ps1
# Purpose: Stop the im-gateway process started by start_im_gateway_mock.ps1
#          (or any cargo-launched im-gateway instance on this host).
# Scope:   Windows PowerShell 5.1+ / PowerShell 7+
# Does NOT: kill unrelated processes; only im-gateway by image name.
# Author:  Mavis 接手 agent per DEC-008 (2026-08-31)
# ============================================================================

[CmdletBinding()]
param(
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

$procs = Get-Process -Name "im-gateway" -ErrorAction SilentlyContinue
if (-not $procs) {
    Write-Host "[stop_im_gateway] no im-gateway process found"
    exit 0
}

foreach ($p in $procs) {
    Write-Host "[stop_im_gateway] stopping pid=$($p.Id)"
    if ($Force) {
        Stop-Process -Id $p.Id -Force
    } else {
        Stop-Process -Id $p.Id
    }
}

# Wait for ports to be released (best-effort, max 5s).
$ports = @(18080, 18081, 19001)
$deadline = (Get-Date).AddSeconds(5)
while ((Get-Date) -lt $deadline) {
    $busy = $ports | Where-Object {
        Test-NetConnection -ComputerName 127.0.0.1 -Port $_ -InformationLevel Quiet -WarningAction SilentlyContinue
    }
    if (-not $busy) { break }
    Start-Sleep -Milliseconds 500
}

Write-Host "[stop_im_gateway] DONE"
