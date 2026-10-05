# check-openapi.ps1 -- OpenAPI 3.1 spec vs actix route registration drift gate
#
# WHAT IT PROVES
#   docs/api/openapi.json claims to be bidirectionally consistent with the code.
#   This script re-derives the route table from the Rust sources and compares it to
#   the spec in BOTH directions, and pins every error code the spec cites to the
#   authoritative registry in aux-03 section B.
#
# DESIGN RULES (each one bought by a bug during development; do not simplify away)
#   1. Fail closed. 0 extracted routes => exit 2, never exit 0. A parser that matches
#      nothing silently turns the gate into a no-op that always passes.
#   2. Hand-verified baselines. Counted off the tree by hand and asserted.
#   3. Print every finding before exiting. Aborting on the first one hides the rest,
#      which are usually the same root cause.
#   4. Never match inside comments or string literals. See Sanitize-ForParsing -- the first
#      version of this script counted parens in raw text and a single stray ")" inside
#      a // comment (mod.rs) popped a scope early, so 7 routes lost their /auth prefix.
#   5. One regex per construct. The first version scanned a 400-char window after the
#      path argument to find the HTTP verb, which happily matched the NEXT route's verb.
#   6. Case sensitivity is decided per operator: -ceq / -cnotcontains / (?i) where it
#      matters, because -match and -like are case-INsensitive by default in PowerShell.
#
# EXIT CODES
#   0 = all checks passed
#   1 = at least one violation (gate failed)
#   2 = could not parse inputs (fail closed, NOT a pass)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent $PSScriptRoot
$SpecPath = Join-Path $RepoRoot 'docs/api/openapi.json'
$Aux03 = Join-Path $RepoRoot 'docs/templates/04-detailed-design/auxiliary/aux-03-error-code-registry.md'
$MainPath = Join-Path $RepoRoot 'crates/im-gateway/src/main.rs'
$HttpMod = Join-Path $RepoRoot 'crates/im-gateway/src/http/mod.rs'
$WsRouter = Join-Path $RepoRoot 'crates/im-gateway/src/ws/router.rs'

# --- Hand-verified baselines -------------------------------------------------
# spec: 21 paths / 24 operations.
# code: 24 registrations = 3 root (healthz, readyz, metrics)
#            + 5 auth + 10 conversations + 3 friends + 2 me + 1 ws
#       (the ws scope registers "" and "/", which normalize to one operation)
$BaselineSpecPaths = 21
$BaselineSpecOperations = 24
$BaselineCodeRoutes = 24
# aux-03 section B: 21 codes, already pinned to im_common::ErrorCode by
# scripts/check-error-codes.ps1 -- so citing a code here is citing the Rust enum.
$BaselineErrorCodes = 21

$violations = New-Object System.Collections.Generic.List[string]
function Add-Violation { param([string]$Message) $violations.Add($Message) }

$HTTP_METHODS = @('get', 'post', 'put', 'patch', 'delete', 'head', 'options', 'trace')

function Normalize-Path {
    param([string]$Path)
    $p = $Path -replace '\s', ''
    if ($p.Length -gt 1) { $p = $p.TrimEnd('/') }
    if ([string]::IsNullOrEmpty($p)) { $p = '/' }
    return $p
}

# --- Neutralize comments, keep string values ---------------------------------
# Two hard-won requirements, pulling in opposite directions:
#   * parens must not be counted inside comments -- a single stray ")" in a // comment
#     in mod.rs popped a scope early and stripped 7 routes of their /auth prefix
#   * string CONTENTS must survive, because the path, the scope prefix and the HTTP
#     verb are all read out of them (an earlier version blanked strings to "" and the
#     delegation lookup then read scope("") and found nothing)
# A single left-to-right alternation resolves the ordering ambiguity: at each
# position a block comment is tried before a line comment before a string, so a "/*"
# or "//" sitting inside a string cannot truncate the file.
function Sanitize-ForParsing {
    param([string]$Text)
    return [regex]::Replace(
        $Text,
        '(?s)(/\*.*?\*/)|(//[^\n]*)|("(?:\\.|[^"\\])*")',
        {
            param($m)
            if ($m.Groups[1].Success) { return (' ' * $m.Value.Length) }   # block comment: drop
            if ($m.Groups[2].Success) { return (' ' * $m.Value.Length) }   # line comment: drop
            return ($m.Value -replace '[()]', ' ')                          # string: keep content
        }
    )
}

# --- Route extraction --------------------------------------------------------
# A single regex captures both the path and the verb, so a route can never borrow
# another route's verb. Paren-depth stack decides when a scope() stops applying:
#   pushed at the depth where `scope("X")` was seen, popped once a ")" brings the
#   depth back BELOW that. That is what makes
#     .service(web::scope("/v1").configure(http::configure))
#     .route("/healthz", web::get().to(health::healthz))
#   parse correctly -- /healthz is scanned after the pop and correctly does NOT
#   inherit /v1. "Last scope seen wins" text scanning gets this exactly backwards.
function Get-RoutesFromFile {
    param([string]$Path, [string]$Prefix)

    $found = @{}
    if (-not (Test-Path -LiteralPath $Path)) { throw "route source not found: $Path" }
    $text = Sanitize-ForParsing ([System.IO.File]::ReadAllText($Path))

    $rx = [regex]'(?:\bscope\s*\(\s*"(?<scope>[^"]*)"\s*\))|(?:\.route\s*\(\s*"(?<path>[^"]*)"\s*,\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*web::(?<verb>get|post|put|patch|delete|head|options)\s*\(\s*\))'

    # Paren depth at every character offset, counting EVERY paren in the file.
    # An earlier version only counted the parens between matches, which undercounted:
    # the regex consumes the scope's own parens and the route's web::post() parens, so
    # the .route( opening paren read as a stray close and popped the live scope. The
    # auth scope then lost its prefix on 4 of its 5 routes. Full accounting removes
    # the accounting mismatch entirely.
    $depthAt = New-Object int[] $text.Length
    $d = 0
    for ($k = 0; $k -lt $text.Length; $k++) {
        $depthAt[$k] = $d
        $c = $text[$k]
        if ($c -eq '(') { $d++ } elseif ($c -eq ')') { $d-- }
    }

    $scopeStack = New-Object System.Collections.Generic.List[object]
    foreach ($m in $rx.Matches($text)) {
        $here = $depthAt[$m.Index]

        while ($scopeStack.Count -gt 0 -and $here -lt $scopeStack[$scopeStack.Count - 1].Depth) {
            $scopeStack.RemoveAt($scopeStack.Count - 1)
        }

        if ($m.Groups['scope'].Success) {
            $scopeStack.Add([pscustomobject]@{ Prefix = $m.Groups['scope'].Value; Depth = $here })
            continue
        }

        $scopePrefix = ''
        if ($scopeStack.Count -gt 0) { $scopePrefix = $scopeStack[$scopeStack.Count - 1].Prefix }
        $full = Normalize-Path ($Prefix + $scopePrefix + $m.Groups['path'].Value)
        $found[($m.Groups['verb'].Value.ToUpperInvariant()) + ' ' + $full] = $true
    }
    return $found
}

function Get-Delegation {
    param([string]$Path)
    $text = Sanitize-ForParsing ([System.IO.File]::ReadAllText($Path))
    $rx = [regex]'\bscope\s*\(\s*"(?<prefix>[^"]*)"\s*\)\s*\.\s*configure\s*\(\s*(?<mod>[A-Za-z_][A-Za-z0-9_]*)::(?<fn>[A-Za-z_][A-Za-z0-9_]*)\s*\)'
    $ms = $rx.Matches($text)
    if ($ms.Count -ne 1) {
        throw ("expected exactly 1 scope(...).configure(...) delegation, found " + $ms.Count + " -- failing closed")
    }
    return [pscustomobject]@{ Prefix = $ms[0].Groups['prefix'].Value; Module = $ms[0].Groups['mod'].Value }
}

function Test-NestedRouter {
    param([string]$Path)
    $text = Sanitize-ForParsing ([System.IO.File]::ReadAllText($Path))
    return [regex]::IsMatch($text, '(?:[A-Za-z_][A-Za-z0-9_]*::)+router::configure\s*\(\s*cfg\s*\)')
}

# =============================================================================
# 1. Parse the spec
# =============================================================================
if (-not (Test-Path -LiteralPath $SpecPath)) { Write-Host "FAIL: spec not found: $SpecPath"; exit 2 }
$rawSpec = [System.IO.File]::ReadAllText($SpecPath)
try { $spec = $rawSpec | ConvertFrom-Json } catch { Write-Host ("FAIL: spec is not valid JSON: " + $_.Exception.Message); exit 2 }
Write-Host ("spec parsed: openapi=" + $spec.openapi)

# =============================================================================
# 2. Enumerate spec operations
# =============================================================================
$specOps = @{}
$specPathKeys = @()
$operationIds = @{}

foreach ($pathProp in $spec.paths.PSObject.Properties) {
    $specPathKeys += $pathProp.Name
    $pathKey = Normalize-Path $pathProp.Name
    foreach ($mProp in $pathProp.Value.PSObject.Properties) {
        if ($HTTP_METHODS -cnotcontains $mProp.Name) { continue }
        $op = $mProp.Value
        $key = $mProp.Name.ToUpperInvariant() + ' ' + $pathKey
        if ($specOps.ContainsKey($key)) { Add-Violation ("duplicate operation in spec: " + $key) }
        $specOps[$key] = $op

        $oid = $op.operationId
        if ([string]::IsNullOrWhiteSpace($oid)) {
            Add-Violation ("operation has no operationId: " + $key)
        } elseif ($operationIds.ContainsKey($oid)) {
            Add-Violation ("duplicate operationId '" + $oid + "': " + $operationIds[$oid] + " and " + $key)
        } else { $operationIds[$oid] = $key }

        # A WebSocket upgrade answers 101, not 2xx; everything else must offer a 2xx.
        # Read via PSObject.Properties: under Set-StrictMode, touching a missing
        # property throws instead of yielding $null.
        $isWs = $null -ne $op.PSObject.Properties['x-websocket']
        $acceptable = if ($isWs) { '^[123]\d\d$' } else { '^2\d\d$' }
        $hasSuccess = $false
        $respProp = $op.PSObject.Properties['responses']
        if ($null -ne $respProp) {
            foreach ($rProp in $respProp.Value.PSObject.Properties) {
                if ($rProp.Name -cmatch $acceptable) { $hasSuccess = $true }
            }
        }
        if (-not $hasSuccess) {
            Add-Violation ("operation has no acceptable success response (" + $acceptable + "): " + $key)
        }

        # Every bearer-protected operation can answer 503: the Bearer extractor in
        # http/auth.rs returns json_response(ServiceUnavailable) on ANY request when
        # the token service is not configured. That path is reachable independently of
        # the handler, so it belongs in the contract, not just in the handler's own
        # error list. (/readyz also has a 503, but it is not bearer-protected.)
        if ($null -ne $op.PSObject.Properties['security'] -and -not $isWs) {
            $has503 = ($null -ne $respProp) -and ($null -ne $respProp.Value.PSObject.Properties['503'])
            if (-not $has503) { Add-Violation ("bearer-protected operation does not document a 503 response: " + $key) }
            $codesProp0 = $op.PSObject.Properties['x-error-codes']
            $cites503Code = ($null -ne $codesProp0) -and (@($codesProp0.Value) -ccontains 'SERVICE_UNAVAILABLE')
            if (-not $cites503Code) { Add-Violation ("bearer-protected operation does not cite SERVICE_UNAVAILABLE: " + $key) }
        }
    }
}

$specPathKeys = @($specPathKeys | Sort-Object -Unique)
if ($specPathKeys.Count -ne $BaselineSpecPaths) {
    Add-Violation ("spec path count is " + $specPathKeys.Count + ", expected " + $BaselineSpecPaths + " (hand-verified baseline; if the API really changed, update it in the same commit)")
}
if ($specOps.Count -ne $BaselineSpecOperations) {
    Add-Violation ("spec operation count is " + $specOps.Count + ", expected " + $BaselineSpecOperations + " (hand-verified baseline)")
}

# =============================================================================
# 3. Extract code routes
# =============================================================================
$codeRoutes = @{}
try {
    $delegation = Get-Delegation -Path $MainPath
    if ($delegation.Module -cne 'http') {
        throw ("main.rs delegates to '" + $delegation.Module + "::configure', but this gate only reads http/mod.rs -- update the extractor rather than weakening the check")
    }
    Write-Host ("delegation: http::configure mounted at " + $delegation.Prefix)

    foreach ($kv in (Get-RoutesFromFile -Path $MainPath -Prefix '').GetEnumerator()) { $codeRoutes[$kv.Key] = $true }
    foreach ($kv in (Get-RoutesFromFile -Path $HttpMod -Prefix $delegation.Prefix).GetEnumerator()) { $codeRoutes[$kv.Key] = $true }
    if (Test-NestedRouter -Path $HttpMod) {
        foreach ($kv in (Get-RoutesFromFile -Path $WsRouter -Prefix $delegation.Prefix).GetEnumerator()) { $codeRoutes[$kv.Key] = $true }
    } else {
        throw ("http/mod.rs no longer delegates to a ws router module, but ws/router.rs is expected to register routes -- the extraction set just changed shape, failing closed")
    }
} catch {
    Write-Host ("FAIL: could not extract routes from source: " + $_.Exception.Message)
    exit 2
}

if ($codeRoutes.Count -eq 0) { Write-Host "FAIL: route extraction produced 0 routes -- the parser is broken, not the API"; exit 2 }
if ($codeRoutes.Count -ne $BaselineCodeRoutes) {
    Add-Violation ("code route count is " + $codeRoutes.Count + ", expected " + $BaselineCodeRoutes + " (a route was added/removed/renamed in the sources)")
}

# =============================================================================
# 4. Bidirectional route comparison
# =============================================================================
foreach ($k in ($codeRoutes.Keys | Sort-Object)) {
    if (-not $specOps.ContainsKey($k)) { Add-Violation ("route exists in code but not in spec: " + $k) }
}
foreach ($k in ($specOps.Keys | Sort-Object)) {
    if (-not $codeRoutes.ContainsKey($k)) { Add-Violation ("route documented in spec but not registered in code: " + $k) }
}

# =============================================================================
# 5. Error codes must exist in aux-03 section B
# =============================================================================
if (-not (Test-Path -LiteralPath $Aux03)) { Write-Host "FAIL: error-code registry not found: $Aux03"; exit 2 }
$auxText = [System.IO.File]::ReadAllText($Aux03)
# (?m) is required: without it ^ anchors to string start only and the table yields 0 rows.
$codeRx = [regex]'(?m)^\|\s*`(?<code>[A-Z][A-Z_]*)`\s*\|\s*\d{3}'
$registry = @{}
foreach ($m in $codeRx.Matches($auxText)) { $registry[$m.Groups['code'].Value] = $true }
if ($registry.Count -ne $BaselineErrorCodes) {
    Add-Violation ("aux-03 error-code table has " + $registry.Count + " codes, expected " + $BaselineErrorCodes + " (the registry is the authority; see check-error-codes.ps1)")
}

$referenced = @{}
foreach ($k in $specOps.Keys) {
    $op = $specOps[$k]
    $codesProp = $op.PSObject.Properties['x-error-codes']
    if ($null -eq $codesProp) { Add-Violation ("operation is missing the x-error-codes extension: " + $k); continue }
    foreach ($c in @($codesProp.Value)) {
        if (-not $registry.ContainsKey($c)) {
            Add-Violation ("operation " + $k + " cites error code '" + $c + "' which is not in aux-03 section B")
        }
        $referenced[$c] = $true
    }
}
$uncited = @($registry.Keys | Where-Object { -not $referenced.ContainsKey($_) } | Sort-Object)

# aux-03 defines some codes that must NOT appear as an error response, so demanding
# they be cited would be wrong. The only such code today:
#   IDEMPOTENCY_CONFLICT -- aux-03 line 93: REST returns HTTP 200 with the original
#   message_id and WS returns ack.ok=true; line 167 of the same file literally throws
#   "internal: IDEMPOTENCY_CONFLICT on 200". Citing it in an error list would document
#   a response the spec forbids.
# The list is hand-pinned and asserted, so it cannot grow quietly, and each entry must
# still exist in the registry (if aux-03 renames it, we go red instead of ignoring it).
$SuccessSemanticCodes = @('IDEMPOTENCY_CONFLICT')
if ($SuccessSemanticCodes.Count -ne 1) {
    Add-Violation ("success-semantic exemption list has " + $SuccessSemanticCodes.Count + " entries, expected 1 -- justify any addition in the same commit")
}
foreach ($c in $SuccessSemanticCodes) {
    if (-not $registry.ContainsKey($c)) {
        Add-Violation ("exempted code '" + $c + "' no longer exists in aux-03 section B -- update the exemption, do not delete the check")
    }
    if ($referenced.ContainsKey($c)) {
        Add-Violation ("'" + $c + "' is exempt as a success semantic but an operation cites it as an error code")
    }
}
$uncited = @($uncited | Where-Object { $SuccessSemanticCodes -cnotcontains $_ })
if ($uncited.Count -gt 0) { Add-Violation ("aux-03 codes never cited by any operation: " + ($uncited -join ', ')) }

# =============================================================================
# 6. $ref integrity
# =============================================================================
$refRx = [regex]'"\$ref"\s*:\s*"(?<ptr>#/components/(?<kind>schemas|parameters|responses)/(?<name>[^"]+))"'
foreach ($m in $refRx.Matches($rawSpec)) {
    $section = $spec.components.($m.Groups['kind'].Value)
    if ($null -eq $section) { Add-Violation ("`$ref points at a missing section: " + $m.Groups['ptr'].Value); continue }
    if ($null -eq $section.PSObject.Properties[$m.Groups['name'].Value]) {
        Add-Violation ("`$ref does not resolve: " + $m.Groups['ptr'].Value)
    }
}

# =============================================================================
# 7. No placeholders left behind
# =============================================================================
$placeholderRx = [regex]'(?i)\b(TODO|FIXME|TBD|lorem ipsum)\b|example\.(com|org)|\?\?\?|\u5F85\u5B9A|\bXXX\b'
foreach ($m in $placeholderRx.Matches($rawSpec)) { Add-Violation ("spec contains a placeholder token: '" + $m.Value + "'") }

# =============================================================================
# Report
# =============================================================================
Write-Host ("code routes: " + $codeRoutes.Count + " | spec operations: " + $specOps.Count + " | spec paths: " + $specPathKeys.Count + " | aux-03 codes: " + $registry.Count + " | operationIds: " + $operationIds.Count)

if ($violations.Count -gt 0) {
    Write-Host ''
    Write-Host ("FAILED with " + $violations.Count + " violation(s):")
    foreach ($v in $violations) { Write-Host ("  - " + $v) }
    exit 1
}
Write-Host 'OK: spec, routes and error-code registry agree'
exit 0
