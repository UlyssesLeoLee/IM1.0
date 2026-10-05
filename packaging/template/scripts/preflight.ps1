#Requires -Version 7.0
<#
.SYNOPSIS
    im1.0 启动前配置自检（preflight）。

.DESCRIPTION
    为什么需要这个脚本：im-gateway 配置出错时只会打印

        [im-gateway] config load failed: internal error

    不告诉你是哪个变量、也没告诉你值哪里不对（根因是
    `crates/im-common/src/error.rs:232` 的 `#[error("internal error")]`
    漏了 `{0}` 占位符，figment 的完整诊断被 Display 丢掉了）。
    那个诊断**不能**直接放出来：figment 解析 `IM_JWT_SIGNING_KEYS` 失败时
    会回显输入值，而输入值里就是签名密钥原文。

    所以本脚本在**进进程之前**用 PowerShell 自己做一遍校验，
    逐项报出可操作的结论，且**任何情况下都不打印变量的值**。

.PARAMETER EnvFile
    .env 文件路径。默认取当前目录下的 .env。文件不存在时会退回读进程环境变量。

.PARAMETER SkipBinaryCheck
    跳过「真跑一次二进制确认配置能加载」这一步。

.EXAMPLE
    pwsh -File scripts/preflight.ps1
    pwsh -File scripts/preflight.ps1 -EnvFile D:\im1.0\.env
#>
[CmdletBinding()]
param(
    [string] $EnvFile = '.env',
    [switch] $SkipBinaryCheck
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# 中文输出在 codepage 936 的传统 conhost 下会花屏。显式切到 UTF-8 输出编码。
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$script:Failures = [System.Collections.Generic.List[string]]::new()
$script:Warnings = [System.Collections.Generic.List[string]]::new()
$script:Checks   = 0

function Ok   { param([string]$m) $script:Checks++; Write-Host "  [ OK ] $m" -ForegroundColor Green }
function Warn { param([string]$m) $script:Checks++; $script:Warnings.Add($m); Write-Host "  [WARN] $m" -ForegroundColor Yellow }
function Bad  { param([string]$m) $script:Checks++; $script:Failures.Add($m); Write-Host "  [FAIL] $m" -ForegroundColor Red }

# ---------------------------------------------------------------------------
# 读取 .env —— 只取「变量名 -> 值」，不打印值
# ---------------------------------------------------------------------------
function Read-DotEnv {
    param([string] $Path)
    $map = [ordered]@{}
    if (-not (Test-Path -LiteralPath $Path)) { return $map }
    foreach ($raw in [System.IO.File]::ReadAllLines($Path, [System.Text.UTF8Encoding]::new($false))) {
        $line = $raw.Trim()
        if ($line -eq '' -or $line.StartsWith('#')) { continue }
        if ($line -notmatch '^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*)$') { continue }
        $name = $Matches[1]
        $val  = $Matches[2].Trim()
        # 去掉可能存在的成对引号
        if ($val.Length -ge 2 -and (($val[0] -eq '"' -and $val[-1] -eq '"') -or ($val[0] -eq "'" -and $val[-1] -eq "'"))) {
            $val = $val.Substring(1, $val.Length - 2)
        }
        $map[$name] = $val
    }
    return $map
}

# ---------------------------------------------------------------------------
# StrictMode 安全的属性读取
#
# 为什么需要：Set-StrictMode -Version Latest 下，访问一个**不存在**的属性是
# 终止错误，不是返回 $null。JSON 少一个键（{"kind":"nats"} 没有 nats_url、
# 密钥项没有 kid）就会让脚本**当场崩掉**，后面的检查全都不跑，operator 看到的是
# PowerShell 堆栈而不是诊断 —— 一个会崩的检查比没有检查更坏。
# 本函数对「属性不存在」返回 $null，调用方照常走判断分支。
# ---------------------------------------------------------------------------
function Get-Prop {
    param($Object, [string] $Name)
    if ($null -eq $Object) { return $null }
    $p = $Object.PSObject.Properties[$Name]
    if ($null -eq $p) { return $null }
    return $p.Value
}

# ---------------------------------------------------------------------------
# 严格的 JSON 校验
#
# ## 为什么**不能**用 ConvertFrom-Json 当门禁
#
# 实测 PowerShell 7.6 的 ConvertFrom-Json 是**宽松**的, 以下全部被它接受:
#
#   {a:1}          无引号键
#   {'a':1}        单引号
#   {"a":1,}       尾随逗号
#   [{"a":1}       **截断, 缺右方括号**
#   ''             空串
#
# 而真正决定服务能不能起来的是 Rust 侧的 `serde_json` + figment(严格)。
# 用一个**比被测对象更宽松**的解析器当门禁, 后果是 preflight 放行、服务拒绝 ——
# 那比没有 preflight 更坏: 它给出的���是「已验证」的假信号。
#
# 故语法校验一律走 System.Text.Json(与 serde_json 同为严格 RFC 8259)。
# 校验通过后才用 ConvertFrom-Json 取值, 那一步只求方便, 不承担判定。
#
# ## 错误信息只取位置, 不取内容
#
# System.Text.Json 的报错形如 "LineNumber: 0 | BytePositionInLine: 7",
# 只含位置不含输入片段, 可安全展示。绝不回显 $Text 本身 ——
# $Text 对 IM_JWT_SIGNING_KEYS 而言就是密钥原文。
# ---------------------------------------------------------------------------
function Test-JsonStrict {
    param([string] $Text)
    try {
        $doc = [System.Text.Json.JsonDocument]::Parse($Text)
        $doc.Dispose()
        return $true, $null
    }
    catch {
        $m = $_.Exception.Message
        $pos = $null
        if ($m -match 'LineNumber:\s*(\d+)\s*\|\s*BytePositionInLine:\s*(\d+)') {
            $pos = "第 $($Matches[1]) 行第 $($Matches[2]) 字节"
        }
        elseif ($m -match '(?i)at\s+line[:\s]+\d+') { $pos = $Matches[0] }
        return $false, $pos
    }
}

Write-Host ''
Write-Host '=== im1.0 配置自检 (preflight) ===' -ForegroundColor Cyan
Write-Host "配置文件: $EnvFile"

$envMap = Read-DotEnv -Path $EnvFile
if ($envMap.Count -eq 0) {
    Write-Host '  (未读到任何变量，将改用当前进程环境变量)' -ForegroundColor DarkGray
    foreach ($e in [System.Environment]::GetEnvironmentVariables('Process').GetEnumerator()) {
        $envMap[$e.Key] = [string]$e.Value
    }
}

# ---------------------------------------------------------------------------
# 0. 先报出「已知错误的变量名」—— 这些在本仓历史文档里出现过
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[0] 已知错误的变量名（出现即警告）' -ForegroundColor Cyan
$badNames = @{
    'IM_EVENT_PUBLISHER_KIND'     = '不存在此字段。事件配置是**一个** JSON 对象: IM_EVENT_PUBLISHER={"kind":"stub"}'
    'IM_EVENT_PUBLISHER_NATS_URL' = '同上。nats_url 写在 IM_EVENT_PUBLISHER 的 JSON 里'
    'IM__JWT__SIGNING__KEYS'      = '双下划线是 figment 的嵌套分隔符, 本仓单层字段用单下划线: IM_JWT_SIGNING_KEYS'
    'IM__REFRESH__PEPPER'         = '同上: IM_REFRESH_PEPPER'
    'IM__HTTP__PORT'              = '同上: IM_HTTP_PORT'
    'IM_DATABASE_URL'             = '字段叫 postgres_url -> IM_POSTGRES_URL'
    'IM_REFRESH_TOKEN_PEPPER'     = '字段叫 refresh_pepper -> IM_REFRESH_PEPPER'
    'DATABASE_URL'                = 'im-gateway 不读它（仅测试代码用）'
}
foreach ($k in $badNames.Keys) {
    if ($envMap.Contains($k)) { Warn "$k 已设置 —— $($badNames[$k])" }
}
if ($script:Warnings.Count -eq 0) { Ok '未发现错误的变量名' }

# ---------------------------------------------------------------------------
# 1. 四个必填项
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[1] 必填项（AppConfig 里没有 serde default，缺一个就启动失败）' -ForegroundColor Cyan
$required = @('IM_POSTGRES_URL', 'IM_JWT_SIGNING_KEYS', 'IM_REFRESH_PEPPER', 'IM_EVENT_PUBLISHER')
foreach ($k in $required) {
    if (-not $envMap.Contains($k))      { Bad "$k 未设置（必填）" }
    elseif ([string]::IsNullOrWhiteSpace($envMap[$k])) { Bad "$k 存在但为空" }
    else { Ok "$k 已设置（长度 $($envMap[$k].Length)，不显示内容）" }
}

# ---------------------------------------------------------------------------
# 2. IM_POSTGRES_URL 形态
# ---------------------------------------------------------------------------
if ($envMap.Contains('IM_POSTGRES_URL') -and $envMap['IM_POSTGRES_URL']) {
    if ($envMap['IM_POSTGRES_URL'] -match '^(postgres|postgresql)://[^:]+:[^@]+@[^/]+/.+$') { Ok 'IM_POSTGRES_URL 形态正确' }
    else { Bad 'IM_POSTGRES_URL 形态不对，应为 postgres://user:password@host:port/dbname' }
    if ($envMap['IM_POSTGRES_URL'] -match 'CHANGE_ME') { Warn 'IM_POSTGRES_URL 里还留着 CHANGE_ME 占位符' }
}

# ---------------------------------------------------------------------------
# 3. IM_JWT_SIGNING_KEYS 结构
# ---------------------------------------------------------------------------
if ($envMap.Contains('IM_JWT_SIGNING_KEYS') -and $envMap['IM_JWT_SIGNING_KEYS']) {
    $good, $pos = Test-JsonStrict -Text $envMap['IM_JWT_SIGNING_KEYS']
    if (-not $good) {
        Bad "IM_JWT_SIGNING_KEYS 不是合法 JSON$([string]$(if($pos){" ($pos)"}))（不显示内容，以免泄露密钥）"
    }
    else {
        $keys = @($envMap['IM_JWT_SIGNING_KEYS'] | ConvertFrom-Json)
        if ($keys.Count -eq 0) { Bad 'IM_JWT_SIGNING_KEYS 是空数组，至少要 1 项' }
        else {
            $noKid = @($keys | Where-Object { -not (Get-Prop $_ 'kid') })
            $noKey = @($keys | Where-Object { -not (Get-Prop $_ 'key') })
            if ($noKid.Count -gt 0) { Bad "$($noKid.Count) 项缺 kid（kid 会写进 token 头，不能空）" }
            if ($noKey.Count -gt 0) { Bad "$($noKey.Count) 项缺 key" }
            $short = @($keys | Where-Object {
                $k = Get-Prop $_ 'key'
                $k -and ([string]$k).Length -lt 32
            })
            if ($short.Count -gt 0) { Warn "$($short.Count) 项 key 短于 32 字节，建议加长" }
            $placeholder = @($keys | Where-Object {
                $k = Get-Prop $_ 'key'
                $k -and ([string]$k) -match 'CHANGE_ME'
            })
            if ($placeholder.Count -gt 0) { Bad "$($placeholder.Count) 项 key 仍是 CHANGE_ME 占位符" }
            # `active` 缺省时 SigningKeyConfig 的 default_true 会当成 true，
            # 故只有**显式** false 才算未启用。
            $active = @($keys | Where-Object { (Get-Prop $_ 'active') -ne $false })
            if ($active.Count -eq 0) { Bad '没有任何 active=true 的密钥，无法签发 token' }
            else { Ok "IM_JWT_SIGNING_KEYS 共 $($keys.Count) 项，其中 $($active.Count) 项 active（不显示内容）" }
        }
    }
}

# ---------------------------------------------------------------------------
# 4. IM_REFRESH_PEPPER
# ---------------------------------------------------------------------------
if ($envMap.Contains('IM_REFRESH_PEPPER') -and $envMap['IM_REFRESH_PEPPER']) {
    if ($envMap['IM_REFRESH_PEPPER'] -match 'CHANGE_ME') { Bad 'IM_REFRESH_PEPPER 仍是 CHANGE_ME 占位符' }
    elseif ($envMap['IM_REFRESH_PEPPER'].Length -lt 16) { Warn "IM_REFRESH_PEPPER 长度仅 $($envMap['IM_REFRESH_PEPPER'].Length)，建议至少 16 字节" }
    else { Ok "IM_REFRESH_PEPPER 长度 $($envMap['IM_REFRESH_PEPPER'].Length)（不显示内容）" }
}

# ---------------------------------------------------------------------------
# 5. IM_EVENT_PUBLISHER 结构（这是最容易配错的一项）
# ---------------------------------------------------------------------------
if ($envMap.Contains('IM_EVENT_PUBLISHER') -and $envMap['IM_EVENT_PUBLISHER']) {
    $good, $pos = Test-JsonStrict -Text $envMap['IM_EVENT_PUBLISHER']
    if (-not $good) {
        Bad "IM_EVENT_PUBLISHER 不是合法 JSON$([string]$(if($pos){" ($pos)"}))。注意 JSON 的键要带双引号: {`"kind`":`"stub`"}"
    }
    else {
        $ep = $envMap['IM_EVENT_PUBLISHER'] | ConvertFrom-Json
        $kind = Get-Prop $ep 'kind'
        $natsUrl = Get-Prop $ep 'nats_url'
        # JSON 合法但不是对象（写成了字符串 / 数组）时 Get-Prop 一律返回 $null，
        # 于是落到「缺 kind」这条 —— 结论仍然正确，不会崩。
        if (-not $kind) { Bad 'IM_EVENT_PUBLISHER 缺 kind 字段（取值 stub 或 nats）' }
        elseif ($kind -notin @('stub', 'nats')) { Bad "IM_EVENT_PUBLISHER.kind = '$kind'，只接受 stub 或 nats" }
        elseif ($kind -eq 'nats') {
            if (-not $natsUrl) { Bad 'kind=nats 但没给 nats_url（事件会发不出去）' }
            else { Ok "IM_EVENT_PUBLISHER: kind=nats（nats_url 长度 $(([string]$natsUrl).Length)）" }
        }
        else { Ok 'IM_EVENT_PUBLISHER: kind=stub（事件被有意丢弃，不是故障）' }
    }
}

# ---------------------------------------------------------------------------
# 6. 可选项
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '[2] 可选项' -ForegroundColor Cyan
if ($envMap.Contains('IM_HTTP_PORT')) {
    $p = 0
    if ([int]::TryParse($envMap['IM_HTTP_PORT'], [ref]$p) -and $p -ge 1 -and $p -le 65535) { Ok "IM_HTTP_PORT = $p" }
    else { Bad "IM_HTTP_PORT = '$($envMap['IM_HTTP_PORT'])' 不是 1-65535 的整数" }
}
else { Ok 'IM_HTTP_PORT 未设置，用默认 8080' }

if ($envMap.Contains('IM_SERVER_SECRETS')) {
    $good, $pos = Test-JsonStrict -Text $envMap['IM_SERVER_SECRETS']
    if ($good) { Ok "IM_SERVER_SECRETS 是合法 JSON（$($envMap['IM_SERVER_SECRETS'].Length) 字节，不显示内容）" }
    else { Bad "IM_SERVER_SECRETS 不是合法 JSON$([string]$(if($pos){" ($pos)"}))（不显示内容）" }
}
else { Ok 'IM_SERVER_SECRETS 未设置 —— S2S token exchange 将不可用，其余功能不受影响' }

# ---------------------------------------------------------------------------
# 7. 真跑一次二进制，确认它真的能把配置加载起来
# ---------------------------------------------------------------------------
if (-not $SkipBinaryCheck) {
    Write-Host ''
    Write-Host '[3] 用真二进制确认配置可加载' -ForegroundColor Cyan
    $exe = Join-Path $PSScriptRoot '..\bin\im-gateway.exe'
    if (-not (Test-Path -LiteralPath $exe)) {
        Warn "找不到 $exe，跳过二进制检查"
    }
    else {
        # 把 .env 里的变量注入本进程（仅本子 shell 可见，不改系统环境）
        $psi = [System.Diagnostics.ProcessStartInfo]::new()
        $psi.FileName = (Resolve-Path -LiteralPath $exe).Path
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError  = $true
        $psi.UseShellExecute = $false
        foreach ($k in $envMap.Keys) { $psi.Environment[$k] = $envMap[$k] }
        $p = [System.Diagnostics.Process]::Start($psi)
        # 配置失败会立即退出；成功则会继续等 PG，最多给它 6 秒
        $exited = $p.WaitForExit(6000)
        $stdout = $p.StandardOutput.ReadToEnd()
        $stderr = $p.StandardError.ReadToEnd()
        if (-not $exited) { $p.Kill($true); $p.WaitForExit() }

        if ($stdout -match 'im-gateway starting' -or $stderr -match 'im-gateway starting') {
            Ok '二进制已通过配置加载阶段（后续停在数据库连接属正常，preflight 不测数据库）'
        }
        elseif ($stderr -match 'config load failed') {
            Bad '二进制报 config load failed —— 本脚本的逐项校验里应有更具体的 FAIL，若这里是唯一一条请把上面全部输出贴给维护者'
        }
        else {
            Warn "二进制未给出可判定的配置结论（exit=$(if($exited){$p.ExitCode}else{'仍在运行'}))"
        }
    }
}

# ---------------------------------------------------------------------------
# 汇总
# ---------------------------------------------------------------------------
Write-Host ''
Write-Host '=== 结果 ===' -ForegroundColor Cyan
Write-Host "  检查项: $script:Checks    失败: $($script:Failures.Count)    警告: $($script:Warnings.Count)"
if ($script:Failures.Count -gt 0) {
    Write-Host ''
    Write-Host '必须修复:' -ForegroundColor Red
    foreach ($f in $script:Failures) { Write-Host "  - $f" -ForegroundColor Red }
    Write-Host ''
    exit 1
}
Write-Host '  配置自检通过。' -ForegroundColor Green
if ($script:Warnings.Count -gt 0) {
    Write-Host ''
    Write-Host '警告（不阻塞，但建议看一眼）:' -ForegroundColor Yellow
    foreach ($w in $script:Warnings) { Write-Host "  - $w" -ForegroundColor Yellow }
}
exit 0
