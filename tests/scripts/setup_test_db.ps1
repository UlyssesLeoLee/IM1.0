# ============================================================================
# setup_test_db.ps1
# Purpose: Create a per-developer test PG database, apply migrations, and
#          load fixture rows. Idempotent — safe to re-run.
# Scope:   Windows PowerShell 5.1+ / PowerShell 7+
# Does NOT: drop im1dev, drop the user's existing data, install software.
# Author:  Mavis 接手 agent per DEC-008 (2026-08-31)
# ============================================================================

[CmdletBinding()]
param(
    [string]$DbName      = "im1test_$($env:USERNAME -replace '[^A-Za-z0-9]','_')",
    [string]$DbUser      = "postgres",
    [string]$DbHost      = "127.0.0.1",
    [int]   $DbPort      = 5555,
    [string]$MigrationsDir = (Join-Path $PSScriptRoot ".." ".." "migrations"),
    [string]$FixturesDir   = (Join-Path $PSScriptRoot ".." "data" "sql")
)

$ErrorActionPreference = 'Stop'

# ---------------------------------------------------------------------------
# 0. Preflight
# ---------------------------------------------------------------------------
Write-Host "[setup_test_db] target db : $DbName on ${DbHost}:${DbPort} as $DbUser"

if (-not (Get-Command psql -ErrorAction SilentlyContinue)) {
    throw "psql not on PATH. Install PostgreSQL client tools or set PATH."
}
if (-not (Test-Path $MigrationsDir)) {
    throw "Migrations dir not found: $MigrationsDir"
}
if (-not (Test-Path $FixturesDir)) {
    throw "Fixtures dir not found:  $FixturesDir"
}

# ---------------------------------------------------------------------------
# 1. createdb IF NOT EXISTS
# ---------------------------------------------------------------------------
$env:PGPASSWORD = $env:IM_TEST_DB_PASSWORD
if (-not $env:PGPASSWORD) {
    Write-Warning "IM_TEST_DB_PASSWORD not set; falling back to empty password (will likely fail unless trust auth)."
}
$env:PGPASSWORD = $env:IM_TEST_DB_PASSWORD  # ensure set for child psql

$exists = psql -h $DbHost -p $DbPort -U $DbUser -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='$DbName';" 2>$null
if ($exists -ne "1") {
    Write-Host "[setup_test_db] creating database $DbName"
    psql -h $DbHost -p $DbPort -U $DbUser -d postgres -c "CREATE DATABASE `"$DbName`";" | Out-Null
} else {
    Write-Host "[setup_test_db] database $DbName already exists; skipping CREATE"
}

# ---------------------------------------------------------------------------
# 2. Run all migrations in lexicographic order
# ---------------------------------------------------------------------------
$migrationFiles = Get-ChildItem -Path $MigrationsDir -Filter "*.sql" | Sort-Object Name
foreach ($mig in $migrationFiles) {
    Write-Host "[setup_test_db] applying $($mig.Name)"
    psql -h $DbHost -p $DbPort -U $DbUser -d $DbName -v ON_ERROR_STOP=1 -f $mig.FullName | Out-Null
}

# ---------------------------------------------------------------------------
# 3. Load fixtures (6 files, FK-ordered)
# ---------------------------------------------------------------------------
$fixtureOrder = @(
    "tenant_fixture.sql",
    "user_fixture.sql",
    "friend_fixture.sql",
    "conversation_fixture.sql",
    "message_fixture.sql",
    "audit_log_fixture.sql"
)
foreach ($name in $fixtureOrder) {
    $path = Join-Path $FixturesDir $name
    if (-not (Test-Path $path)) {
        Write-Warning "missing fixture: $name — skipping"
        continue
    }
    Write-Host "[setup_test_db] loading   $name"
    psql -h $DbHost -p $DbPort -U $DbUser -d $DbName -v ON_ERROR_STOP=1 -f $path | Out-Null
}

# ---------------------------------------------------------------------------
# 4. Smoke-check row counts
# ---------------------------------------------------------------------------
$checks = @(
    "tenants", "games", "environments",
    "users", "device_sessions",
    "friend_requests", "friendships",
    "conversations", "conversation_sequences", "conversation_members", "dm_pairs",
    "messages", "message_reactions",
    "audit_logs"
)
Write-Host "[setup_test_db] row counts:"
foreach ($t in $checks) {
    $n = psql -h $DbHost -p $DbPort -U $DbUser -d $DbName -tAc "SELECT count(*) FROM $t;" 2>$null
    Write-Host ("  {0,-25} {1}" -f $t, ($n -as [string]))
}

Write-Host "[setup_test_db] DONE — db=$DbName"
