#!/usr/bin/env pwsh
# =============================================================================
# check-asyncapi.ps1 - AsyncAPI spec vs im-protocol serde definitions
#
# WHY THIS EXISTS
#   docs/api/asyncapi.json documents the WebSocket frame protocol. Every field
#   set in it was taken from the `serde` derives in im-protocol, NOT from the
#   aux-13 sample JSON (ImplementationSpec is [PROTOCOL-FROZEN] and its samples
#   conflict with the code in several places). That makes the spec a
#   hand-maintained duplicate of the Rust definitions - exactly the shape that
#   silently drifts.
#
# WHAT IT CHECKS
#   1. variant names    : every Rust enum variant has a schema whose
#                         properties.type.const equals the snake_case wire
#                         value, and vice versa (bidirectional).
#   2. field names      : every variant field appears in the schema's
#                         properties, and vice versa (bidirectional).
#   3. required flags   : a field carrying `skip_serializing_if` must NOT be
#                         listed in `required`; a field without it MUST be.
#                         This is the subtle one - see note below.
#   4. $ref resolution : every $ref in the document resolves.
#   5. baselines        : hand-verified counts, so a parse failure that yields
#                         empty sets can never read as "clean".
#
# THE `required` RULE (this is the whole point of check 3)
#   serde gives `Option<T>` three different behaviours, and they produce
#   different JSON:
#     (a) no attribute at all                  -> always emitted, `null` if None
#     (b) `#[serde(default)]` only             -> ALSO always emitted. `default`
#                                                affects deserialization only.
#     (c) `#[serde(default, skip_serializing_if = "...")]` -> key OMITTED entirely
#   So: has skip_serializing_if  => NOT required
#       no  skip_serializing_if  => required
#   Getting this backwards makes the spec describe a shape the server never
#   emits (case c marked required) or demand a field it never sends (case a/b
#   marked optional). Both are wrong, and neither shows up as a runtime error.
#
# FAIL-CLOSED
#   If the Rust parsing yields fewer variants than expected, the script prints
#   what it did find and exits 2. A parser that silently returns "no drift"
#   because it failed to parse is the classic way a gate becomes a constant-pass
#   placeholder.
#
# Pure ASCII, no BOM (repo convention for scripts/).
# =============================================================================

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$RepoRoot = Split-Path -Parent (Split-Path -Parent $PSCommandPath)
$SpecPath = Join-Path $RepoRoot 'docs/api/asyncapi.json'
$WsFramesPath = Join-Path $RepoRoot 'crates/im-protocol/src/ws_frames.rs'
$ContentPath = Join-Path $RepoRoot 'crates/im-protocol/src/content.rs'
$ErrorBodyPath = Join-Path $RepoRoot 'crates/im-protocol/src/error_body.rs'

# Hand-verified baselines (counted by hand against the Rust on 2026-10-05;
# ServerFrame recounted on 2026-10-07 when AuthOk was added).
$ExpectedClientVariants = 8
# 11 since 2026-10-07: `AuthOk` joins the enum. wire is byte-identical to the
# hand-built JSON it replaced; the point is that it is now covered here.
$ExpectedServerVariants = 11
$ExpectedContentVariants = 6
$ExpectedMessages = 19

# Server frames that intentionally have no ServerFrame enum variant.
#
# 2026-10-07: this list is now EMPTY and must stay that way. `auth_ok` used to be
# here because it was built by a raw `serde_json::json!` in im-gateway and had no
# enum variant — which is precisely why it escaped every gate and every contract
# test. It now IS `ServerFrame::AuthOk`, so it is checked like any other frame.
#
# Adding a frame to this list makes the bidirectional check skip it entirely.
# That is a hole, not a convenience: keep the list minimal and justify each
# entry in writing. If it is empty, say so out loud rather than deleting the
# variable, so the next person knows the case existed and was resolved.
$DeclaredOnlyServerFrames = @()
if ($DeclaredOnlyServerFrames.Count -eq 0) {
    Write-Host 'note: DeclaredOnlyServerFrames is empty — every documented server frame has a ServerFrame variant.'
}

# Fields whose required-ness is governed by a DIFFERENT Rust type than the enum
# being checked. Key format: "<wire>.<field>".
#
# `auth.req_id` - im_protocol::ClientFrame::Auth declares `req_id: Uuid`
#   (non-Option, so "required"). But the production path never parses the first
#   frame as that enum: handler.rs:331 calls
#   `serde_json::from_str::<AuthFrame>(text)`, and handler.rs:119-124 declares
#   `AuthFrame::Auth { #[serde(default)] req_id: Option<Uuid>, .. }` - a
#   Deserialize-only type with no skip_serializing_if. So the field is genuinely
#   optional on the wire, and handler.rs:381-384 echoes it back as `null` when
#   omitted. Marking it required would document a shape the server does not
#   accept; the spec correctly models it as optional.
#
# This list is a hole if it grows, so its size is asserted below.
$RequiredFieldExceptions = @{
    'auth.req_id' = 'parsed by handler.rs AuthFrame (Option<Uuid>), not by im_protocol::ClientFrame::Auth'
}
if ($RequiredFieldExceptions.Count -ne 1) {
    Write-Host "FAIL-CLOSED: RequiredFieldExceptions has $($RequiredFieldExceptions.Count) entries, expected 1."
    Write-Host "          Every new entry must come with a written justification in this file."
    exit 2
}

$violations = [System.Collections.Generic.List[string]]::new()

function Add-Violation {
    param([string]$Message)
    $violations.Add($Message)
}

# StrictMode makes `$obj.MissingProperty` a terminating error, and plenty of
# schemas legitimately have no `properties` (Uuid, UnixMillis, the oneOf
# MessageContent). Read through this so "absent" is $null instead of a crash.
function Get-Prop {
    param($Obj, [string]$Name)
    if ($null -eq $Obj) { return $null }
    $p = $Obj.PSObject.Properties[$Name]
    if ($null -eq $p) { return $null }
    return $p.Value
}

# Ordered list of a schema's property names, or @() when it has none.
function Get-PropNames {
    param($SchemaObj)
    $p = Get-Prop $SchemaObj 'properties'
    if ($null -eq $p) { return @() }
    return @($p.PSObject.Properties.Name)
}

function Get-Required {
    param($SchemaObj)
    $r = Get-Prop $SchemaObj 'required'
    if ($null -eq $r) { return @() }
    return @($r)
}

# -----------------------------------------------------------------------------
# Rust source stripping
# -----------------------------------------------------------------------------
# One left-to-right alternation, same technique as check-openapi.ps1: block
# comments discarded, line comments discarded, string contents PRESERVED with
# only their delimiters blanked. Preserving string contents matters because the
# serde attribute values we must detect are string literals:
#   skip_serializing_if = "is_false"
# If strings were blanked (a natural first attempt), that attribute would become
# invisible and check 3 would degrade into "never fires".
function Remove-RustComments {
    param([string]$Text)
    $sb = [System.Text.StringBuilder]::new($Text.Length)
    $i = 0
    $n = $Text.Length
    while ($i -lt $n) {
        $c = $Text[$i]
        $next = if ($i + 1 -lt $n) { $Text[$i + 1] } else { "`0" }
        if ($c -eq '/' -and $next -eq '/') {
            while ($i -lt $n -and $Text[$i] -ne "`n") { $i++ }
        }
        elseif ($c -eq '/' -and $next -eq '*') {
            $depth = 1
            $i += 2
            while ($i -lt $n -and $depth -gt 0) {
                if ($Text[$i] -eq '/' -and ($i + 1 -lt $n) -and $Text[$i + 1] -eq '*') { $depth++; $i += 2; continue }
                if ($Text[$i] -eq '*' -and ($i + 1 -lt $n) -and $Text[$i + 1] -eq '/') { $depth--; $i += 2; continue }
                $i++
            }
        }
        elseif ($c -eq '"') {
            [void]$sb.Append('"')
            $i++
            while ($i -lt $n) {
                if ($Text[$i] -eq '\\' -and ($i + 1) -lt $n) { [void]$sb.Append($Text[$i]); [void]$sb.Append($Text[$i + 1]); $i += 2; continue }
                [void]$sb.Append($Text[$i])
                if ($Text[$i] -eq '"') { $i++; break }
                $i++
            }
        }
        else {
            [void]$sb.Append($c)
            $i++
        }
    }
    return $sb.ToString()
}

# serde `rename_all = "snake_case"`: insert `_` before every uppercase letter
# that is not at position 0, then lowercase.
#
# LIMITATION (deliberate, not an oversight): serde's own implementation splits
# runs of capitals differently (its `HTTPResponse` -> `http_response` special
# case). None of the current variant names contain consecutive capitals, and
# the variant-count baselines below turn any future rename into a loud failure
# rather than a silent mis-derivation.
function ConvertTo-SnakeCase {
    param([string]$Name)
    $sb = [System.Text.StringBuilder]::new()
    for ($i = 0; $i -lt $Name.Length; $i++) {
        $ch = $Name[$i]
        if ([char]::IsUpper($ch) -and $i -gt 0) { [void]$sb.Append('_') }
        [void]$sb.Append([char]::ToLowerInvariant($ch))
    }
    return $sb.ToString()
}

# Extract `pub enum <Name> { ... }` and return, per variant:
#   Name, Wire, Fields = ordered list of @{ Name; SkipsSerializeIf }
function Get-RustEnum {
    param(
        [string]$Source,
        [string]$EnumName
    )
    $m = [regex]::Match($Source, "pub\s+enum\s+$EnumName\s*\{")
    if (-not $m.Success) { return $null }
    $start = $m.Index + $m.Length - 1
    $depth = 0
    $i = $start
    for (; $i -lt $Source.Length; $i++) {
        if ($Source[$i] -eq '{') { $depth++ }
        elseif ($Source[$i] -eq '}') { $depth--; if ($depth -eq 0) { break } }
    }
    if ($depth -ne 0) { return $null }
    $body = $Source.Substring($start + 1, $i - $start - 1)

    $variants = [System.Collections.Generic.List[object]]::new()
    # Variant header: optional doc comments already stripped, so a name is
    # `Identifier {` (struct variant) or `Identifier,` (unit variant).
    $pattern = [regex]'(?ms)^\s{4}([A-Z][A-Za-z0-9_]*)\s*(\{|,)'
    foreach ($vm in $pattern.Matches($body)) {
        $name = $vm.Groups[1].Value
        $fields = [System.Collections.Generic.List[object]]::new()
        if ($vm.Groups[2].Value -eq '{') {
            $vs = $vm.Index + $vm.Length - 1
            $vd = 0
            $j = $vs
            for (; $j -lt $body.Length; $j++) {
                if ($body[$j] -eq '{') { $vd++ }
                elseif ($body[$j] -eq '}') { $vd--; if ($vd -eq 0) { break } }
            }
            if ($vd -ne 0) { Add-Violation "unbalanced braces in $EnumName::$name"; continue }
            $vbody = $body.Substring($vs + 1, $j - $vs - 1)
            # Field: optional serde attribute lines, then `ident: Type` followed by
            # a comma OR the end of the variant body.
            #
            # The terminator alternation is load-bearing: single-line variants
            # (`RecallMessage { req_id: Uuid, message_id: Uuid }`) have no comma
            # after their LAST field, because the comma that follows belongs to
            # the variant list. A comma-only terminator silently dropped that
            # field, which showed up as "schema documents 'message_id' but Rust
            # has no such field" - a gate bug that reads exactly like a spec bug.
            #
            # LIMITATION: a type containing a comma (`HashMap<String, String>`)
            # would break `[^,\n]`. No such type exists in these enums today; the
            # variant-count baselines turn a future one into a loud failure.
            $fpat = [regex]'(?ms)(#\[serde\((?<attrs>[^\)]*)\)\]\s*)?(?<name>[a-z_][a-z0-9_]*)\s*:\s*[^,\n]+?\s*(?:,|\s*\z)'
            foreach ($fm in $fpat.Matches($vbody)) {
                $attrs = if ($fm.Groups['attrs'].Success) { $fm.Groups['attrs'].Value } else { '' }
                $fields.Add(@{
                    Name               = $fm.Groups['name'].Value
                    SkipsSerializeIf   = $attrs -match 'skip_serializing_if'
                })
            }
        }
        $variants.Add(@{
            Name   = $name
            Wire   = ConvertTo-SnakeCase $name
            Fields = $fields
        })
    }
    return $variants
}

# -----------------------------------------------------------------------------
# Load inputs
# -----------------------------------------------------------------------------
if (-not (Test-Path $SpecPath)) { Write-Host "FAIL: missing $SpecPath"; exit 2 }
if (-not (Test-Path $WsFramesPath)) { Write-Host "FAIL: missing $WsFramesPath"; exit 2 }

$specText = [System.IO.File]::ReadAllText($SpecPath, [System.Text.UTF8Encoding]::new($false, $true))
try {
    $spec = $specText | ConvertFrom-Json -Depth 100
}
catch {
    Write-Host "FAIL: asyncapi.json is not valid JSON: $($_.Exception.Message)"
    exit 2
}
Write-Host "spec parsed: asyncapi=$($spec.asyncapi)"

$wsSource = Remove-RustComments ([System.IO.File]::ReadAllText($WsFramesPath))
$contentSource = Remove-RustComments ([System.IO.File]::ReadAllText($ContentPath))
$errorBodySource = Remove-RustComments ([System.IO.File]::ReadAllText($ErrorBodyPath))

$clientVariants = Get-RustEnum $wsSource 'ClientFrame'
$serverVariants = Get-RustEnum $wsSource 'ServerFrame'
$contentVariants = Get-RustEnum $contentSource 'MessageContent'

# --- fail closed on parse collapse ------------------------------------------
# If the parser broke, these are null or empty and every comparison below would
# trivially pass. Refuse to continue.
foreach ($pair in @(
        @{ Name = 'ClientFrame'; Got = $clientVariants; Want = $ExpectedClientVariants },
        @{ Name = 'ServerFrame'; Got = $serverVariants; Want = $ExpectedServerVariants },
        @{ Name = 'MessageContent'; Got = $contentVariants; Want = $ExpectedContentVariants }
    )) {
    $count = if ($null -eq $pair.Got) { 0 } else { $pair.Got.Count }
    if ($count -ne $pair.Want) {
        Write-Host "FAIL-CLOSED: parsed $($pair.Name) found $count variant(s), expected $($pair.Want)."
        Write-Host "          the Rust parser regressed; NOT reporting drift."
        if ($null -ne $pair.Got) { $pair.Got | ForEach-Object { Write-Host "          saw: $($_.Name) -> $($_.Wire)" } }
        exit 2
    }
}

$schemas = $spec.components.schemas
$schemaNames = @($schemas.PSObject.Properties.Name)

function Get-SchemaFor {
    param($SchemaObj)
    $typeProp = Get-Prop (Get-Prop $SchemaObj 'properties') 'type'
    $const = Get-Prop $typeProp 'const'
    if ($null -eq $const) { return $null }
    return [string]$const
}

# Discriminator index built PER DIRECTION, not globally.
#
# A single global map is wrong: `typing` exists on BOTH sides (ClientFrame::Typing
# and ServerFrame::Typing), so one entry silently overwrote the other and
# ClientFrame::typing got checked against ServerTypingFrame's schema. That is
# how a gate ends up reporting 19 schemas as 18 and then blaming the spec.
#
# The correct source of "which schemas are client-side" is the operation message
# lists - which is also a stronger check, because it means a schema can only
# satisfy a direction if an operation actually references it.
function Get-DiscriminatorsFor {
    param($Operation)
    $map = [ordered]@{}
    foreach ($mr in $Operation.messages) {
        # operation -> message ref -> message object -> payload.$ref -> schema
        $messageName = ($mr.'$ref' -replace '^.*/messages/', '')
        $msg = Get-Prop $spec.components.messages $messageName
        if ($null -eq $msg) {
            Add-Violation "operation references undefined message '$messageName'"
            continue
        }
        $schemaName = (Get-Prop (Get-Prop $msg 'payload') '$ref') -replace '^.*/schemas/', ''
        if ([string]::IsNullOrWhiteSpace($schemaName) -or $schemaNames -notcontains $schemaName) {
            Add-Violation "message '$messageName' has an unresolvable payload schema '$schemaName'"
            continue
        }
        $wire = Get-SchemaFor $schemas.$schemaName
        if ($null -ne $wire) { $map[$wire] = $schemaName }
    }
    return $map
}

$clientDiscriminators = Get-DiscriminatorsFor $spec.operations.clientToServer
$serverDiscriminators = Get-DiscriminatorsFor $spec.operations.serverToClient
Write-Host ("schemas: {0} | client discriminators: {1} | server discriminators: {2}" -f $schemaNames.Count, $clientDiscriminators.Count, $serverDiscriminators.Count)

# -----------------------------------------------------------------------------
# Check 1 + 2 + 3: variants, fields and required flags
# -----------------------------------------------------------------------------
function Test-VariantSet {
    param(
        [object[]]$Variants,
        [string]$Label,
        $Discriminators,
        [string]$Tag,
        [string[]]$AllowedOnlyInSpec
    )
    $seen = [System.Collections.Generic.List[string]]::new()

    foreach ($v in $Variants) {
        $seen.Add($v.Wire)
        if (-not $Discriminators.Contains($v.Wire)) {
            Add-Violation "$Label variant $($v.Name) (wire '$($v.Wire)') has no schema referenced by this operation"
            continue
        }
        $schemaName = $Discriminators[$v.Wire]
        $sc = $schemas.$schemaName

        $props = @(Get-PropNames $sc | Where-Object { $_ -ne $Tag })
        $required = @(Get-Required $sc | Where-Object { $_ -ne $Tag })
        $rustFields = @($v.Fields | ForEach-Object { $_.Name })

        # field names, bidirectional
        foreach ($f in $rustFields) {
            if ($props -notcontains $f) {
                Add-Violation "$Label/$($v.Wire): Rust field '$f' is absent from schema '$schemaName'"
            }
        }
        foreach ($p in $props) {
            if ($rustFields -notcontains $p) {
                Add-Violation "$Label/$($v.Wire): schema '$schemaName' documents property '$p' but Rust has no such field"
            }
        }

        # required flags, bidirectional. This is the load-bearing check.
        foreach ($f in $v.Fields) {
            if ($RequiredFieldExceptions.ContainsKey("$($v.Wire).$($f.Name)")) { continue }
            $isRequired = $required -contains $f.Name
            if ($f.SkipsSerializeIf -and $isRequired) {
                Add-Violation "$Label/$($v.Wire): field '$($f.Name)' has skip_serializing_if (key disappears when None) but is listed in required"
            }
            if ((-not $f.SkipsSerializeIf) -and (-not $isRequired)) {
                Add-Violation "$Label/$($v.Wire): field '$($f.Name)' has no skip_serializing_if (always emitted, null when None) but is NOT in required"
            }
        }
        foreach ($r in $required) {
            if ($rustFields -notcontains $r) {
                Add-Violation "$Label/$($v.Wire): schema '$schemaName' requires '$r' which is not a Rust field"
            }
        }
    }

    # the other direction: a documented discriminator no Rust variant produces
    foreach ($wire in $Discriminators.Keys) {
        if ($seen -contains $wire) { continue }
        if ($null -ne $AllowedOnlyInSpec -and $AllowedOnlyInSpec -contains $wire) { continue }
        Add-Violation "${Label}: schema '$($Discriminators[$wire])' documents type='$wire' but no Rust variant produces it"
    }
}

Test-VariantSet -Variants $clientVariants -Label 'ClientFrame' -Discriminators $clientDiscriminators -Tag 'type'
Test-VariantSet -Variants $serverVariants -Label 'ServerFrame' -Discriminators $serverDiscriminators -Tag 'type' -AllowedOnlyInSpec $DeclaredOnlyServerFrames
# MessageContent discriminates on `kind`, not `type`, so it is not part of the
# discriminator index. Check it separately against its own `kind` const.
foreach ($v in $contentVariants) {
    $schemaName = "MessageContent$(if ($v.Wire -eq 'text') { 'Text' } else { $v.Wire.Substring(0, 1).ToUpper() + $v.Wire.Substring(1) })"
    if ($schemaNames -notcontains $schemaName) {
        Add-Violation "MessageContent variant $($v.Name) (wire '$($v.Wire)'): expected schema '$schemaName' not found"
        continue
    }
    $sc = $schemas.$schemaName
    $kindConst = Get-Prop (Get-Prop (Get-Prop $sc 'properties') 'kind') 'const'
    if ([string]$kindConst -ne $v.Wire) {
        Add-Violation "MessageContent/$($v.Wire): schema '$schemaName' has kind.const='$kindConst'"
    }
    $props = @(Get-PropNames $sc | Where-Object { $_ -ne 'kind' })
    $required = @(Get-Required $sc)
    foreach ($f in $v.Fields) {
        if ($props -notcontains $f.Name) {
            Add-Violation "MessageContent/$($v.Wire): Rust field '$($f.Name)' absent from '$schemaName'"
        }
        $isRequired = $required -contains $f.Name
        if ($f.SkipsSerializeIf -and $isRequired) {
            Add-Violation "MessageContent/$($v.Wire): '$($f.Name)' has skip_serializing_if but is required"
        }
        if ((-not $f.SkipsSerializeIf) -and (-not $isRequired)) {
            Add-Violation "MessageContent/$($v.Wire): '$($f.Name)' has no skip_serializing_if but is not required"
        }
    }
    foreach ($p in $props) {
        if (@($v.Fields | ForEach-Object { $_.Name }) -notcontains $p) {
            Add-Violation "MessageContent/$($v.Wire): schema documents '$p' with no Rust field"
        }
    }
}

# ErrorBody field set
$ebMatch = [regex]::Match($errorBodySource, 'pub\s+struct\s+ErrorBody\s*\{')
if (-not $ebMatch.Success) { Add-Violation 'could not locate ErrorBody struct' }
else {
    $ebSchema = $schemas.ErrorBody
    if ($null -eq $ebSchema) { Add-Violation 'asyncapi.json has no ErrorBody schema' }
    else {
        $ebFields = [System.Collections.Generic.List[string]]::new()
        $ebSkips = [System.Collections.Generic.List[string]]::new()
        $fpat = [regex]'(?ms)(#\[serde\((?<attrs>[^\)]*)\)\]\s*)?pub\s+(?<name>[a-z_][a-z0-9_]*)\s*:'
        # Clamp to the struct's own braces rather than a fixed window: a fixed
        # 2000-char slice throws on any source shorter than that (error_body.rs
        # is ~1.3KB), and it would silently over-read into unrelated structs on
        # a longer one.
        $ebStart = $ebMatch.Index
        $ebDepth = 0
        $ebEnd = $ebStart
        for ($k = $ebStart; $k -lt $errorBodySource.Length; $k++) {
            if ($errorBodySource[$k] -eq '{') { $ebDepth++ }
            elseif ($errorBodySource[$k] -eq '}') { $ebDepth--; if ($ebDepth -eq 0) { $ebEnd = $k; break } }
        }
        $ebBody = $errorBodySource.Substring($ebStart, $ebEnd - $ebStart)
        foreach ($fm in $fpat.Matches($ebBody)) {
            $ebFields.Add($fm.Groups['name'].Value)
            if ($fm.Groups['attrs'].Success -and $fm.Groups['attrs'].Value -match 'skip_serializing_if') {
                $ebSkips.Add($fm.Groups['name'].Value)
            }
        }
        $ebProps = @(Get-PropNames $ebSchema)
        foreach ($f in $ebFields) {
            if ($ebProps -notcontains $f) { Add-Violation "ErrorBody: Rust field '$f' absent from schema" }
        }
        foreach ($p in $ebProps) {
            if ($ebFields -notcontains $p) { Add-Violation "ErrorBody: schema documents '$p' with no Rust field" }
        }
        $ebRequired = @(Get-Required $ebSchema)
        foreach ($f in $ebFields) {
            $isRequired = $ebRequired -contains $f
            if ($ebSkips.Contains($f) -and $isRequired) {
                Add-Violation "ErrorBody: '$f' has skip_serializing_if but is required"
            }
            if ((-not $ebSkips.Contains($f)) -and (-not $isRequired)) {
                Add-Violation "ErrorBody: '$f' has no skip_serializing_if but is not required"
            }
        }
    }
}

# -----------------------------------------------------------------------------
# Check 4: every $ref resolves
# -----------------------------------------------------------------------------
$refCount = 0
$refPattern = [regex]'"\$ref"\s*:\s*"(?<ptr>#/[^"]+)"'
foreach ($rm in $refPattern.Matches($specText)) {
    $refCount++
    $ptr = $rm.Groups['ptr'].Value
    $cur = $spec
    $ok = $true
    foreach ($seg in ($ptr -replace '^#/', '' -split '/')) {
        $seg = $seg -replace '~1', '/' -replace '~0', '~'
        if ($null -eq $cur -or $null -eq $cur.PSObject.Properties[$seg]) { $ok = $false; break }
        $cur = $cur.($seg)
    }
    if (-not $ok) { Add-Violation "unresolvable `$ref: $ptr" }
}
Write-Host "refs checked: $refCount"

# -----------------------------------------------------------------------------
# Check 5: baselines
# -----------------------------------------------------------------------------
$msgCount = @($spec.components.messages.PSObject.Properties.Name).Count
if ($msgCount -ne $ExpectedMessages) {
    Add-Violation "message count is $msgCount, expected $ExpectedMessages"
}
$opCount = 0
foreach ($op in $spec.operations.PSObject.Properties) {
    foreach ($mr in $op.Value.messages) { $opCount++ }
}
Write-Host "messages: $msgCount | operations: $(@($spec.operations.PSObject.Properties.Name).Count) (message refs: $opCount) | schemas: $($schemaNames.Count)"

# Every frame wire value must appear in exactly one operation's message list, so
# no frame is documented but unreachable (and none is listed twice under the
# wrong direction).
$listedSend = @()
$listedReceive = @()
foreach ($mr in $spec.operations.clientToServer.messages) { $listedSend += ($mr.'$ref' -replace '^.*/messages/', '') }
foreach ($mr in $spec.operations.serverToClient.messages) { $listedReceive += ($mr.'$ref' -replace '^.*/messages/', '') }
$dupSend = @($listedSend | Group-Object | Where-Object { $_.Count -gt 1 })
$dupRecv = @($listedReceive | Group-Object | Where-Object { $_.Count -gt 1 })
if ($dupSend.Count -gt 0) { Add-Violation "clientToServer lists duplicate messages: $($dupSend.Name -join ', ')" }
if ($dupRecv.Count -gt 0) { Add-Violation "serverToClient lists duplicate messages: $($dupRecv.Name -join ', ')" }
foreach ($m in $listedSend + $listedReceive) {
    if ($spec.components.messages.PSObject.Properties[$m] -eq $null) {
        Add-Violation "operation references undefined message '$m'"
    }
}

# -----------------------------------------------------------------------------
# Verdict (print everything collected BEFORE exiting - fail closed and loud)
# -----------------------------------------------------------------------------
if ($violations.Count -gt 0) {
    Write-Host ""
    Write-Host "FAIL: asyncapi.json has drifted from im-protocol ($($violations.Count) violation(s)):"
    foreach ($v in $violations) { Write-Host "  - $v" }
    exit 1
}

Write-Host "OK: asyncapi.json matches the im-protocol serde definitions"
exit 0
