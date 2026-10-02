# check-error-codes.ps1 -- guard ErrorCode consistency (2026-10-03)
#
# WHY THIS EXISTS
#   crates/im-common/src/error.rs claimed "CI scans error code strings against the
#   enum via scripts/check_error_codes.sh". That script never existed -- ci.yml
#   L136-137 had already recorded that fact, yet the comment survived.
#   The drift it failed to catch: four comments in
#   crates/im-gateway/src/ws/handler.rs claimed business frames return
#   `UNSUPPORTED_OPERATION` (501, "per aux-13 section 4"). Neither half is true:
#     - the code is NOT in aux-03 section B (the authoritative registry)
#     - aux-13 section 4 is "Prerequisites", not an error-code section
#
# WHAT IS CHECKED (all three are exact, zero-false-positive checks)
#   A. error.rs: every ErrorCode variant has exactly one as_str() wire name,
#      and wire names are unique.
#   B. aux-03-error-code-registry.md section B: the authoritative table's code
#      column matches the enum's wire-name set EXACTLY (no missing, no extra).
#      This is the check that would have caught the fabricated code above, and
#      it also closes the reverse gap: a code added to the enum but not to the
#      registry (aux-03 section B says "any PR adding one must update this table
#      and DetailedDesign").
#   C. every `ErrorCode::X` used outside error.rs names a real variant.
#
# WHAT IS DELIBERATELY NOT CHECKED
#   Error-code-shaped tokens in comments. A first attempt flagged every
#   SCREAMING_SNAKE token with >=2 underscores and produced 228 false positives
#   (IM_HTTP_PORT, MAX_PASSWORD_LEN, CARGO_PKG_VERSION, test fixtures ...).
#   There is no syntactic position that distinguishes "an error code mentioned
#   in prose" from "an env var or const", so this stays a review duty -- noted
#   in docs/gap-ledger.md section 1.6 rather than pretended to be automated.
#
# EXIT CODES
#   0 = clean
#   1 = drift found (details on stdout)
#   2 = could not parse inputs (fail closed -- never silently pass)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
# Forward slashes on purpose: this runs on ubuntu CI too, where backslashes in
# paths are not separators. PowerShell accepts '/' on Windows as well.
$errorFile = Join-Path $repoRoot 'crates/im-common/src/error.rs'
$registryFile = Join-Path $repoRoot 'docs/templates/04-detailed-design/auxiliary/aux-03-error-code-registry.md'
$cratesDir = Join-Path $repoRoot 'crates'

foreach ($f in @($errorFile, $registryFile)) {
    if (-not (Test-Path $f)) { Write-Output "FAIL: missing input $f"; exit 2 }
}

# ---------------------------------------------------------------------------
# A. Parse enum variants + wire names from error.rs
# ---------------------------------------------------------------------------
$src = Get-Content $errorFile -Raw

$variants = @{}
foreach ($m in [regex]::Matches($src, '(?m)^\s{4}([A-Z]\w*),\s*//[^\r\n]*$')) {
    $variants[$m.Groups[1].Value] = $true
}

$wireToVariant = @{}
foreach ($m in [regex]::Matches($src, 'ErrorCode::([A-Z]\w*)\s*=>\s*"([A-Z0-9_]+)"')) {
    $wireToVariant[$m.Groups[2].Value] = $m.Groups[1].Value
}

if ($variants.Count -eq 0 -or $wireToVariant.Count -eq 0) {
    Write-Output "FAIL: could not parse registry from error.rs (variants=$($variants.Count) wire=$($wireToVariant.Count))"
    exit 2
}
Write-Output "A. error.rs: $($variants.Count) variants, $($wireToVariant.Count) wire names"

# ---------------------------------------------------------------------------
# B. Parse aux-03 section B table and compare both directions
# ---------------------------------------------------------------------------
$regText = Get-Content $registryFile -Raw
$secB = [regex]::Match($regText, '(?ms)^## B\..*?(?=^## [C-Z]\.)')
if (-not $secB.Success) { Write-Output "FAIL: could not locate section B in $registryFile"; exit 2 }

$docCodes = @{}
foreach ($m in [regex]::Matches($secB.Value, '(?m)^\|\s*`([A-Z][A-Z0-9_]+)`\s*\|')) {
    $docCodes[$m.Groups[1].Value] = $true
}
if ($docCodes.Count -eq 0) { Write-Output 'FAIL: section B table parsed to zero codes'; exit 2 }
Write-Output "B. aux-03 section B: $($docCodes.Count) codes"

# ---------------------------------------------------------------------------
# C. Scan Rust sources for ErrorCode::X usages
# ---------------------------------------------------------------------------
$rustFiles = Get-ChildItem -Path $cratesDir -Recurse -Filter *.rs -File
$badUsages = @{}
foreach ($f in $rustFiles) {
    if ($f.FullName -eq $errorFile) { continue }
    $text = Get-Content $f.FullName -Raw
    foreach ($m in [regex]::Matches($text, 'ErrorCode::([A-Za-z_]\w*)')) {
        $ident = $m.Groups[1].Value
        if (-not $variants.ContainsKey($ident)) {
            $rel = $f.FullName.Substring($repoRoot.Length + 1)
            $badUsages["$rel : ErrorCode::$ident"] = $true
        }
    }
}
Write-Output "C. scanned $($rustFiles.Count) Rust files"

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
$problems = New-Object System.Collections.Generic.List[string]

foreach ($v in $variants.Keys) {
    if (-not $wireToVariant.Values.Contains($v)) {
        $problems.Add("A. variant '$v' has no as_str() wire name")
    }
}
if ($variants.Count -ne $wireToVariant.Count) {
    $problems.Add("A. count mismatch: $($variants.Count) variants vs $($wireToVariant.Count) wire names")
}

foreach ($w in $wireToVariant.Keys) {
    if (-not $docCodes.ContainsKey($w)) {
        $problems.Add("B. enum has '$w' but aux-03 section B does not document it")
    }
}
foreach ($d in $docCodes.Keys) {
    if (-not $wireToVariant.ContainsKey($d)) {
        $problems.Add("B. aux-03 section B documents '$d' but the enum has no such code")
    }
}

foreach ($k in $badUsages.Keys) { $problems.Add("C. $k") }

if ($problems.Count -gt 0) {
    Write-Output ""
    Write-Output "DRIFT: $($problems.Count) problem(s)"
    $problems | Sort-Object -Unique | ForEach-Object { Write-Output "  - $_" }
    exit 1
}

Write-Output "OK: registry, docs and usages agree ($($wireToVariant.Count) codes)"
exit 0
