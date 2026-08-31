# ============================================================================
# teardown_test_db.ps1
# Purpose: Drop the per-developer test database created by setup_test_db.ps1.
# Scope:   Windows PowerShell 5.1+ / PowerShell 7+
# Safety:  Refuses to drop any db whose name does NOT start with "im1test_".
# Does NOT: drop im1dev or any non-test database.
# Author:  Mavis 接手 agent per DEC-008 (2026-08-31)
# ============================================================================

[CmdletBinding()]
param(
    [string]$DbName = "im1test_$($env:USERNAME -replace '[^A-Za-z0-9]','_')",
    [string]$DbUser = "postgres",
    [string]$DbHost = "127.0.0.1",
    [int]   $DbPort = 5555,
    [switch]$Force
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# Safety guard: only drop names that look like test DBs.
# ---------------------------------------------------------------------------
if ($DbName -notmatch '^im1test_') {
    throw "Refusing to drop db '$DbName' — name must start with 'im1test_'."
}

if (-not $Force) {
    $ans = Read-Host "About to DROP DATABASE $DbName on ${DbHost}:${DbPort}. Type 'yes' to continue"
    if ($ans -ne 'yes') {
        Write-Host "[teardown_test_db] aborted by user"
        exit 0
    }
}

if (-not (Get-Command psql -ErrorAction SilentlyContinue)) {
    throw "psql not on PATH."
}
$env:PGPASSWORD = $env:IM_TEST_DB_PASSWORD

Write-Host "[teardown_test_db] terminating active connections to $DbName"
psql -h $DbHost -p $DbPort -U $DbUser -d postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='$DbName' AND pid<>pg_backend_pid();" | Out-Null

Write-Host "[teardown_test_db] dropping $DbName"
psql -h $DbHost -p $DbPort -U $DbUser -d postgres -c "DROP DATABASE IF EXISTS `"$DbName`";" | Out-Null

Write-Host "[teardown_test_db] DONE"
