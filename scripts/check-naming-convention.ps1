# check-naming-convention.ps1 -- guard aux-01 naming rules (2026-10-04)
#
# WHY THIS EXISTS
#   ci.yml's lint job had a step literally named "aux-01 naming check" whose
#   body was a single `echo` and whose own comment admitted
#   "placeholder: no automated aux-01 section J naming check is enforced here".
#   That is the same species of defect as the semgrep step fixed in 797b5bb:
#   a gate that can never fail. aux-01 section J ("tool enforcement") claims
#   naming violations block CI; for Rust/DB/proto that claim was untrue.
#
#   aux-01 section J also prescribes `clippy::naming`, `sqlfluff`, `buf lint`
#   and `openapi-spec-validator`. None of those are wired up. This script
#   covers the DB and proto layers (and Rust *file* naming) with what can be
#   decided exactly and without a compiler. See the ci.yml clippy step for
#   the Rust identifier layer.
#
# WHAT IS CHECKED (each rule cites its aux-01 section)
#   D.1 migration file name       ^\d{4}_[a-z0-9_]+\.sql$
#   D.2 migration seq gapless     0001..N, no gaps, no duplicates
#   D.3 table name                ^[a-z][a-z0-9_]*$
#   D.4 column name               ^[a-z][a-z0-9_]*$   (SQL keywords excluded)
#   D.5 index name                ^(idx|uniq)_[a-z0-9_]+$
#   D.6 named constraint          ^(chk|uniq)_[a-z0-9_]+$
#   D.7 trigger name              ^trg_[a-z0-9_]+$
#   E.1 proto package             ^im\.<svc>\.v<major>$
#   E.2/E.3 service + message     PascalCase
#   E.4 rpc name                  PascalCase
#   E.5 rpc request suffix        ...Request
#       rpc response type         must be a message declared in this proto, or a
#                                 google.protobuf well-known type (see below for
#                                 why the ...Response suffix is NOT enforced)
#   F.1 Rust source file name     ^[a-z][a-z0-9_]*\.rs$
#   F.2 Rust module dir name      ^[a-z][a-z0-9_]*$   (crate root excluded)
#   F.3 proto file name           ^[a-z][a-z0-9_]*\.proto$
#   I.1 no SQL identifier named `status`
#       (aux-01 section I: delivery state must be `delivery_state`;
#        `status` is reserved for users.state)
#   I.2 no SQL object named room/chat
#       (aux-01 section G "Critical Rule": room/channel/chat are forbidden as
#        IM business object names; conversation is the single source term)
#
# BASELINE FLOOR -- WHY IT IS NOT PARANOIA
#   A regex that stops matching reports zero objects. Zero violations then
#   looks identical to a clean repository, so the gate would pass while
#   checking nothing. That is worse than having no gate at all, and it is not
#   hypothetical: the first draft of this script's table parser omitted
#   `IF NOT EXISTS` and silently extracted 0 columns out of 39.
#   aux-01 section H makes migrations append-only, so these inventories only
#   grow. A count BELOW the floor therefore means the parser regressed, not
#   that objects were removed -- so we fail closed (exit 2) and say so.
#   If you ever legitimately remove an object, update the floor with a note.
#
# WHAT IS DELIBERATELY NOT CHECKED (measured, not assumed)
#   - aux-01 section I "no CJK / pinyin identifiers". Deciding this needs a
#     real Rust lexer (comments and string literals must be stripped first).
#     Measured: a naive regex over this repo yields 8 hits, ALL of them
#     Chinese prose inside comments -- zero real violations, all false
#     positives. A gate that cries wolf on comments is not a gate.
#     clippy has no lint for this; stays a review duty.
#   - aux-01 section E "REST paths must start with /v1/". Measured: this repo
#     legitimately contains both `/v1/friends` and the bare fragment
#     `/friends`, because actix nests `web::scope("/v1")` around
#     `web::scope("/friends")`. Resolving scopes to full paths needs
#     understanding the App tree, not a regex. Stays a review duty.
#   - aux-01 section D "index name spells out every column" (its own example
#     is uniq_users_environment_id_external_identity). Measured: 9 existing
#     indexes abbreviate (uniq_users_env_extid, uniq_messages_idem,
#     idx_messages_conversation_seq, ...). Enforcing it would be 9 findings
#     on day one, so the deviations are recorded in docs/gap-ledger.md
#     rather than pretended absent.
#   - aux-01 section D "table names are plural", and section 6 "new tables
#     must be registered in the section G glossary". Plurality needs a
#     lexicon; glossary coverage is currently incomplete (5 of 14 tables).
#     Both are doc-owner items, not mechanical ones.
#   - aux-01 section B Rust identifier rules beyond the defaults (Error suffix,
#     no error-variant suffix, etc.). Needs an AST. The naming lints that are
#     on by default are already denied by the existing `clippy -- -D warnings`
#     step -- measured, not assumed: injecting `fn Badly_Named_Function_...()`
#     into im-common makes that step exit 101. aux-01 section J asks for
#     `-D clippy::naming`, but **no such lint exists** (`E0602: unknown lint`),
#     and because -D warnings promotes unknown_lints to a hard error, wiring
#     section J verbatim would break the build. See docs/gap-ledger.md 1.26.
#   - aux-01 section E "gRPC response type must end in Response". Enforcing it
#     would rename 5 live rpcs and break the wire contract; see the E.5 comment.
#
# EXIT CODES
#   0 = clean
#   1 = violation found (details on stdout)
#   2 = could not parse inputs, or a count fell below its floor
#       (fail closed -- never silently pass)

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
# Forward slashes on purpose: this runs on ubuntu CI too, where backslashes
# in paths are not separators. PowerShell accepts '/' on Windows as well.
$cratesDir    = Join-Path $repoRoot 'crates'
$migrationsDir = Join-Path $repoRoot 'migrations'
$protoDir     = Join-Path $repoRoot 'crates/im-proto/proto'

$problems = New-Object System.Collections.Generic.List[string]

function Add-Problem([string] $msg) { $problems.Add($msg) }

# Fail closed with a reason. Never `exit 0` on an unparsed input.
# Any violation already collected is printed FIRST: a floor trip is often a
# *consequence* of an earlier problem, and swallowing the real diagnostic in
# favour of "could not parse" sends the reader after the wrong bug. Measured:
# renaming 0003_....sql to 0003_....SQL makes the file stop being a migration,
# which drops the table count and trips the floor -- but the actual defect is
# the extension, and that message must survive.
function Stop-Parse([string] $what, $got) {
    if ($problems.Count -gt 0) {
        Write-Output ("ALSO REPORTED ({0} problem(s) found before the input stopped being parseable):" -f $problems.Count)
        $problems | Sort-Object -Unique | ForEach-Object { Write-Output "  - $_" }
        Write-Output ''
    }
    Write-Output "FAIL: could not parse $what (got $got). Refusing to report 0 violations -- that would be indistinguishable from a clean tree."
    exit 2
}

foreach ($d in @($cratesDir, $migrationsDir, $protoDir)) {
    if (-not (Test-Path $d)) { Write-Output "FAIL: missing input $d"; exit 2 }
}

# ===========================================================================
# D.1 / D.2 -- migrations
# ===========================================================================
# Enumerated explicitly rather than with -Filter '*.sql' on purpose:
# Get-ChildItem -Filter is case-INSENSITIVE on Windows but case-SENSITIVE on
# Linux, so a migration renamed to `0003_....SQL` would be enumerated and
# checked on a dev box while being invisible on the ubuntu runner -- the gate
# would pass on exactly the machine that runs CI. Selecting with a
# case-sensitive comparison makes both platforms agree.
$migDir = New-Object System.Collections.Generic.List[System.IO.FileInfo]
foreach ($f in (Get-ChildItem -Path $migrationsDir -File | Sort-Object Name)) {
    $migDir.Add($f)
}
$migs = @($migDir | Where-Object { $_.Name -clike '*.sql' })
if ($migs.Count -eq 0) { Stop-Parse 'migration files (migrations/ holds no *.sql)' 0 }

# A numbered file that is not .sql is a migration that the toolchain will not
# pick up, so it is reported. Non-numbered files (a README, say) are not.
foreach ($f in $migDir) {
    if ($f.Name -clike '*.sql') { continue }
    if ($f.Name -match '^[0-9]') {
        Add-Problem ("D.1 '{0}' looks like a migration but is not a .sql file (case matters: the toolchain will skip it)" -f $f.Name)
    }
}

$seqs = New-Object System.Collections.Generic.List[int]
foreach ($f in $migs) {
    $m = [regex]::Match($f.Name, '^(\d{4})_[a-z0-9_]+\.sql$')
    if (-not $m.Success) {
        Add-Problem ("D.1 migration name '{0}' is not <4-digit seq>_<snake_case>.sql" -f $f.Name)
        continue
    }
    $seqs.Add([int]$m.Groups[1].Value)
}
Write-Output ("D.1/D.2 migrations: {0} files, seq {1}..{2}" -f $migs.Count, ($seqs | Measure-Object -Minimum).Minimum, ($seqs | Measure-Object -Maximum).Maximum)

# Gapless from 0001, no duplicates. sqlx tolerates a gap, aux-01 section D
# does not: the sequence number is the ordering contract humans read.
$dupes = $seqs | Group-Object | Where-Object { $_.Count -gt 1 }
foreach ($d in $dupes) {
    Add-Problem ("D.2 migration sequence {0:D4} is used by {1} files" -f [int]$d.Name, $d.Count)
}
$distinct = $seqs | Sort-Object -Unique
for ($i = 1; $i -le ($distinct | Measure-Object -Maximum).Maximum; $i++) {
    if ($distinct -notcontains $i) { Add-Problem ("D.2 migration sequence {0:D4} is missing (gap before the next migration)" -f $i) }
}

# ===========================================================================
# D.3..D.7 + I.1 + I.2 -- SQL object names
# ===========================================================================
$sql = New-Object System.Text.StringBuilder
foreach ($f in $migs) { $null = $sql.AppendLine((Get-Content $f.FullName -Raw -Encoding UTF8)) }
$sqlText = $sql.ToString()

$tables   = New-Object System.Collections.Generic.List[string]
$columns  = New-Object System.Collections.Generic.List[string]
$indexes  = New-Object System.Collections.Generic.List[string]
$namedCon = New-Object System.Collections.Generic.List[string]
$triggers = New-Object System.Collections.Generic.List[string]

# Table-level clauses share the indented-column shape, so they are removed by
# name. This is an explicit list on purpose: the first draft used an
# "identifier is ALL UPPERCASE" heuristic, which would also swallow a real
# column if anyone ever adds one (e.g. a `STATUS`-style column). An auditable
# deny-list cannot hide a violation; a shape heuristic can.
$clauseKeywords = @('CONSTRAINT', 'PRIMARY', 'UNIQUE', 'CHECK', 'FOREIGN', 'EXCLUDE', 'LIKE')

foreach ($m in [regex]::Matches($sqlText, '(?is)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z0-9_]+)\s*\((.*?)\n\);')) {
    $tables.Add($m.Groups[1].Value)
    foreach ($c in [regex]::Matches($m.Groups[2].Value, '(?im)^\s{2,}([A-Za-z_][A-Za-z0-9_]*)\s+[A-Za-z]+')) {
        $name = $c.Groups[1].Value
        if ($clauseKeywords -ccontains $name) { continue }
        if (-not $columns.Contains($name)) { $columns.Add($name) }
    }
}
foreach ($m in [regex]::Matches($sqlText, '(?im)^\s*CREATE\s+(?:UNIQUE\s+)?INDEX\s+(?:IF\s+NOT\s+EXISTS\s+)?([A-Za-z0-9_]+)')) {
    $indexes.Add($m.Groups[1].Value)
}
foreach ($m in [regex]::Matches($sqlText, '(?im)^\s*CONSTRAINT\s+([A-Za-z0-9_]+)\s+(?:UNIQUE|CHECK|FOREIGN|PRIMARY|EXCLUDE)')) {
    $namedCon.Add($m.Groups[1].Value)
}
foreach ($m in [regex]::Matches($sqlText, '(?im)^\s*CREATE\s+(?:OR\s+REPLACE\s+)?TRIGGER\s+([A-Za-z0-9_]+)')) {
    $triggers.Add($m.Groups[1].Value)
}

# Baseline floor. See the header: a drop below any of these means a regex
# stopped matching, not that the repository shrank.
# Verified against the migrations by hand on 2026-10-04, not guessed: there
# are 19 lines containing CREATE INDEX but one of them is commented out
# (idx_messages_content_fts, reserved for full-text search), so 18 are live.
# Named constraints were counted off the CREATE TABLE bodies: 3 chk_ + 3 uniq_.
$floor = [ordered]@{
    'tables'   = 14
    'columns'  = 39
    'indexes'  = 18
    'namedCon' = 6
    'triggers' = 2
}
$got = [ordered]@{
    'tables'   = $tables.Count
    'columns'  = $columns.Count
    'indexes'  = $indexes.Count
    'namedCon' = $namedCon.Count
    'triggers' = $triggers.Count
}
foreach ($k in $floor.Keys) {
    if ($got[$k] -lt $floor[$k]) { Stop-Parse ("SQL objects (floor for '{0}': expected >= {1})" -f $k, $floor[$k]) $got[$k] }
}
Write-Output ("D.3-D.7 SQL objects: {0} tables, {1} columns, {2} indexes, {3} named constraints, {4} triggers" -f $tables.Count, $columns.Count, $indexes.Count, $namedCon.Count, $triggers.Count)

foreach ($t in $tables)   { if ($t -cnotmatch '^[a-z][a-z0-9_]*$') { Add-Problem "D.3 table name '$t' is not snake_case" } }
foreach ($c in $columns)  { if ($c -cnotmatch '^[a-z][a-z0-9_]*$') { Add-Problem "D.4 column name '$c' is not snake_case" } }
foreach ($i in $indexes)  { if ($i -cnotmatch '^(idx|uniq)_[a-z0-9_]+$') { Add-Problem "D.5 index name '$i' must be idx_<table>_<columns> or uniq_<table>_<columns>" } }
foreach ($c in $namedCon) { if ($c -cnotmatch '^(chk|uniq)_[a-z0-9_]+$') { Add-Problem "D.6 named constraint '$c' must be chk_<table>_<column> or uniq_<table>_<columns>" } }
foreach ($t in $triggers) { if ($t -cnotmatch '^trg_[a-z0-9_]+$') { Add-Problem "D.7 trigger name '$t' must be trg_<table>_<action>" } }

# I.1 -- `status` may not name anything in SQL. aux-01 section I reserves
# delivery state for `delivery_state` and allows `status` only conceptually as
# users.state (which the schema spells `state`, so no column is named status).
$allSqlNames = @($tables) + @($columns) + @($indexes) + @($namedCon) + @($triggers)
foreach ($n in $allSqlNames) {
    if ($n -match '(?i)(^|_)status($|_)') { Add-Problem "I.1 SQL identifier '$n' uses the forbidden `status` (use delivery_state)" }
    if ($n -match '(?i)room|chat') { Add-Problem "I.2 SQL object '$n' uses room/chat (IM1.0 uses `conversation`)" }
}

# ===========================================================================
# E.1..E.5 -- proto
# ===========================================================================
$protoFiles = Get-ChildItem -Path $protoDir -Filter '*.proto' -File
if ($protoFiles.Count -eq 0) { Stop-Parse 'proto files' 0 }
$protoText = (($protoFiles | ForEach-Object { Get-Content $_.FullName -Raw -Encoding UTF8 }) -join "`n")

$pkgs = @([regex]::Matches($protoText, '(?m)^\s*package\s+([\w.]+)\s*;') | ForEach-Object { $_.Groups[1].Value })
if ($pkgs.Count -eq 0) { Stop-Parse 'proto package' 0 }
foreach ($p in $pkgs) {
    if ($p -cnotmatch '^im\.[a-z][a-z0-9]*\.v[0-9]+$') { Add-Problem "E.1 proto package '$p' must be im.<service>.v<major>" }
}

$svcs = @([regex]::Matches($protoText, '(?m)^\s*service\s+(\w+)') | ForEach-Object { $_.Groups[1].Value })
$msgs = @([regex]::Matches($protoText, '(?m)^\s*message\s+(\w+)') | ForEach-Object { $_.Groups[1].Value })
$rpcs = @([regex]::Matches($protoText, '(?m)^\s*rpc\s+(\w+)\s*\(\s*([\w.]+)\s*\)\s*returns\s*\(\s*([\w.]+)\s*\)'))
if ($svcs.Count -eq 0) { Stop-Parse 'proto services' 0 }
if ($msgs.Count -eq 0) { Stop-Parse 'proto messages' 0 }
if ($rpcs.Count -eq 0) { Stop-Parse 'proto rpcs' 0 }

# Baseline floor, counted by hand on 2026-10-04: core.proto declares 1 package,
# 1 service, 31 messages and 22 rpcs. api grows, so the floor is "never fewer".
$protoFloor = @{ 'packages' = $pkgs.Count; 'services' = $svcs.Count; 'messages' = 31; 'rpcs' = 22 }
$protoGot   = [ordered]@{ 'packages' = $pkgs.Count; 'services' = $svcs.Count; 'messages' = $msgs.Count; 'rpcs' = $rpcs.Count }
foreach ($k in $protoFloor.Keys) {
    if ($protoGot[$k] -lt $protoFloor[$k]) { Stop-Parse ("proto objects (floor for '{0}': expected >= {1})" -f $k, $protoFloor[$k]) $protoGot[$k] }
}
Write-Output ("E.1-E.5 proto: {0} package(s), {1} service(s), {2} message(s), {3} rpc(s)" -f $pkgs.Count, $svcs.Count, $msgs.Count, $rpcs.Count)

foreach ($s in $svcs) { if ($s -cnotmatch '^[A-Z][A-Za-z0-9]*$') { Add-Problem "E.2 proto service '$s' is not PascalCase" } }
foreach ($m in $msgs) { if ($m -cnotmatch '^[A-Z][A-Za-z0-9]*$') { Add-Problem "E.3 proto message '$m' is not PascalCase" } }
foreach ($r in $rpcs) {
    $name = $r.Groups[1].Value
    $req  = $r.Groups[2].Value
    $resp = $r.Groups[3].Value
    if ($name -cnotmatch '^[A-Z][A-Za-z0-9]*$') { Add-Problem "E.4 proto rpc '$name' is not PascalCase" }
    if ($req -cnotmatch 'Request$') { Add-Problem "E.5 rpc $name request type '$req' must end in Request" }

    # The response type is NOT required to end in Response. aux-01 section E
    # says "gRPC message | PascalCase + Request/Response", but enforcing that
    # literally is wrong twice over:
    #   - `google.protobuf.Empty` is protobuf's own type, not a project message;
    #     aux-01 cannot govern it, and wrapping it would be un-idiomatic.
    #   - the other non-conforming returns hand a domain type straight back
    #     (SendMessage -> Message, GetMe -> User, ExchangeToken -> TokenPair,
    #     CreateConversation/GetConversation -> Conversation). Renaming those
    #     changes the wire contract, which is the spec owner's call, not a
    #     mechanical naming fix. Recorded in docs/gap-ledger.md.
    # So E.5 checks the property that IS decidable and does catch a real bug
    # class: every rpc must name a type that actually exists.
    $isWellKnown = $resp -clike 'google.protobuf.*'
    if (-not $isWellKnown -and $msgs -notcontains $resp) {
        Add-Problem "E.5 rpc $name returns '$resp', which is neither a message declared in this proto nor a google.protobuf well-known type"
    }
}

# ===========================================================================
# F.1..F.3 -- file and directory names
# ===========================================================================
$rsFiles = Get-ChildItem -Path $cratesDir -Recurse -Filter '*.rs' -File
if ($rsFiles.Count -eq 0) { Stop-Parse 'Rust source files' 0 }
foreach ($f in $rsFiles) {
    if ($f.Name -cnotmatch '^[a-z][a-z0-9_]*\.rs$') { Add-Problem "F.1 Rust file '$($f.Name)' is not snake_case" }
}
$protoAll = Get-ChildItem -Path $cratesDir -Recurse -Filter '*.proto' -File
foreach ($f in $protoAll) {
    if ($f.Name -cnotmatch '^[a-z][a-z0-9_]*\.proto$') { Add-Problem "F.3 proto file '$($f.Name)' is not snake_case" }
}
# F.2 -- module directories under each crate's src/. The crate root itself
# (im-gateway, im-testkit, ...) is a *Cargo package* name, which Cargo
# mandates as kebab-case; it is not a Rust module path, so it is excluded.
$moduleDirs = 0
foreach ($crate in (Get-ChildItem -Path $cratesDir -Directory)) {
    $src = Join-Path $crate.FullName 'src'
    if (-not (Test-Path $src)) { continue }
    foreach ($d in (Get-ChildItem -Path $src -Recurse -Directory)) {
        $moduleDirs++
        if ($d.Name -cnotmatch '^[a-z][a-z0-9_]*$') { Add-Problem "F.2 module dir '$($d.Name)' under $($crate.Name) is not snake_case" }
    }
}
if ($moduleDirs -eq 0) { Stop-Parse 'Rust module directories' 0 }
Write-Output ("F.1-F.3 files: {0} .rs, {1} .proto, {2} module dirs" -f $rsFiles.Count, $protoAll.Count, $moduleDirs)

# ===========================================================================
# Report
# ===========================================================================
if ($problems.Count -gt 0) {
    Write-Output ""
    Write-Output ("VIOLATION: {0} problem(s)" -f $problems.Count)
    $problems | Sort-Object -Unique | ForEach-Object { Write-Output "  - $_" }
    exit 1
}

Write-Output ''
Write-Output ('OK: aux-01 section D/E/F/I naming rules hold across {0} migrations, {1} tables, {2} columns, {3} indexes, {4} triggers, {5} rpcs, {6} Rust files' -f $migs.Count, $tables.Count, $columns.Count, $indexes.Count, $triggers.Count, $rpcs.Count, $rsFiles.Count)
exit 0
