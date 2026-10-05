#Requires -Version 7.0
<#
.SYNOPSIS
    preflight.ps1 的变异回归测试 —— 门禁必须被喂过失败输入才算门禁。

.DESCRIPTION
    为什么要有这个：`scripts/preflight.ps1` 是接入方启动服务前唯一的自查手段。
    如果它「永远绿」或者「因为崩掉而变红」，operator 得到的都是错误信号 ——
    前者让人以为配置没问题，后者让人看到 PowerShell 堆栈而不是诊断。

    本脚本做两件事：

    1. **对照组**：一份正确配置必须 exit 0。
    2. **变异组**：6 份各自只坏在一处的配置必须 exit 1，且必须给出**具体**的
       FAIL 行（不是靠「脚本崩了」拿到 exit 1 —— 崩掉也是一种 exit 1，但那是
       假信号：它说明后面的检查全都没跑）。

    另有一条**泄露断言**：把全部输出拼起来，检查密钥原文 / pepper 原文
    有没有出现在里面。preflight 的整个存在理由就是「不打印值」，
    这条断言若破了，preflight 比没有 preflight 更危险。

.EXAMPLE
    pwsh -File scripts/test-preflight.ps1
#>
[CmdletBinding()]
param(
    [switch] $KeepArtifacts
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$repo  = Split-Path -Parent $PSScriptRoot
$preflight = Join-Path $repo 'packaging\template\scripts\preflight.ps1'
if (-not (Test-Path -LiteralPath $preflight)) { throw "找不到 $preflight" }

$work = Join-Path $repo 'target\preflight-test'
if (Test-Path -LiteralPath $work) {
    & "$env:USERPROFILE\.minimax\bin\mavis-trash.cmd" -- $work
    if ($LASTEXITCODE -ne 0) { throw "清理旧测试目录失败 (exit=$LASTEXITCODE)" }
}
New-Item -ItemType Directory -Path $work -Force | Out-Null

# 这两个值用作「绝不能出现在输出里」的探针
$PROBE_KEY    = 'PROBEKEYMATERIAL_0123456789ABCDEF'
$PROBE_PEPPER = 'PROBEPEDPERVALUE_0123456789'

$base = @"
IM_POSTGRES_URL=postgres://im:pw@127.0.0.1:5432/im1
IM_JWT_SIGNING_KEYS=[{"kid":"v1","key":"$PROBE_KEY","active":true}]
IM_REFRESH_PEPPER=$PROBE_PEPPER
IM_EVENT_PUBLISHER={"kind":"stub","nats_url":""}
IM_HTTP_PORT=8080
"@

# 每份变异只坏一处，附「必须出现的那条 FAIL 的关键词」
$cases = @(
    @{ name = 'good';        expect = 0; must = $null;                                              desc = '对照组: 正确配置' }
    @{ name = 'no-event';    expect = 1; must = 'IM_EVENT_PUBLISHER 未设置';                        desc = '缺唯一无默认值的必填项' }
    @{ name = 'bad-json';    expect = 1; must = '不是合法 JSON';                                   desc = 'JWT keys 语法坏(最容易连带泄露的一处)' }
    @{ name = 'no-kid';      expect = 1; must = '缺 kid';                                          desc = '密钥项缺 kid —— StrictMode 下曾让脚本崩掉' }
    @{ name = 'no-nats-url'; expect = 1; must = '没给 nats_url';                                   desc = 'kind=nats 但缺 nats_url —— 同样曾崩掉' }
    @{ name = 'bad-port';    expect = 1; must = '不是 1-65535 的整数';                              desc = '端口越界' }
    @{ name = 'inactive';    expect = 1; must = '没有.*active=true';                                desc = '全部密钥停用, 无法签发' }
    @{ name = 'legacy-names';expect = 0; must = 'IM_EVENT_PUBLISHER_KIND 已设置';                   desc = '历史文档里的错名 -> 必须是 WARN 不是 FAIL(它本身合法, 只是拼错的人要知道)' }
    # 下面 4 个是**「宽松解析器」类**变异: PowerShell 的 ConvertFrom-Json 会放行它们,
    # 但 Rust 侧的 serde_json 会拒绝。preflight 若用宽松解析器, 就会放行一份
    # 服务起不来的配置 —— 那比没有 preflight 更坏, 因为它给的是「已验证」的假信号。
    # 这 4 条是本文件最有价值的用例, 2026-10-06 就是被它们抓出真 bug 的。
    @{ name = 'unquoted-key';expect = 1; must = '不是合法 JSON';                                   desc = 'JSON 无引号键 —— ConvertFrom-Json 放行, serde_json 拒绝' }
    @{ name = 'single-quote'; expect = 1; must = '不是合法 JSON';                                   desc = 'JSON 单引号 —— 同上' }
    @{ name = 'trailing-comma';expect= 1; must = '不是合法 JSON';                                   desc = 'JSON 尾随逗号 —— 同上' }
    @{ name = 'truncated';    expect = 1; must = '不是合法 JSON';                                   desc = 'JSON 截断(缺右方括号) —— 同上, 最阴的一种' }
)

$allOutput = ''
$failed = [System.Collections.Generic.List[string]]::new()

foreach ($c in $cases) {
    $envPath = Join-Path $work "$($c.name).env"
    $content = switch ($c.name) {
        'no-event'    { $base -replace '(?m)^IM_EVENT_PUBLISHER=.*$', '' }
        'bad-json'    { $base -replace '\[\{"kid"', '[{kid' }
        'no-kid'      { $base -replace '"kid":"v1",', '' }
        'no-nats-url' { $base -replace '\{"kind":"stub","nats_url":""\}', '{"kind":"nats"}' }
        'bad-port'    { $base -replace '(?m)^IM_HTTP_PORT=.*$', 'IM_HTTP_PORT=99999' }
        'inactive'    { $base -replace '"active":true', '"active":false' }
        'legacy-names'{ $base + "`nIM_EVENT_PUBLISHER_KIND=nats`nIM_EVENT_PUBLISHER_NATS_URL=nats://x:4222`n" }
        'unquoted-key' { $base -replace '\[\{"kid":"v1"', '[{kid:"v1"' }
        'single-quote' { $base -replace '\[\{"kid":"v1","key":"', "['kid':'v1','key':'" }
        'trailing-comma'{ $base -replace '"active":true\}\]', '"active":true},]' }
        'truncated'    { $base -replace '"active":true\}\]', '"active":true}' }
        default       { $base }
    }
    [System.IO.File]::WriteAllText($envPath, $content, [System.Text.UTF8Encoding]::new($false))

    $out = & pwsh -NoProfile -File $preflight -EnvFile $envPath -SkipBinaryCheck 2>&1 | Out-String
    $code = $LASTEXITCODE
    $allOutput += $out

    $problems = [System.Collections.Generic.List[string]]::new()
    if ($code -ne $c.expect) { $problems.Add("exit=$code, 期望 $($c.expect)") }

    # 「靠崩掉拿到的 exit 1」不算通过
    if ($out -match 'preflight\.ps1:\s' -or $out -match 'CategoryInfo|FullyQualifiedErrorId') {
        $problems.Add('脚本抛了未捕获异常 —— 检查项崩掉会让后续检查全不跑, 是假信号')
    }
    if ($c.must -and $out -notmatch $c.must) {
        $problems.Add("输出里找不到应有的结论: /$($c.must)/")
    }
    if ($c.name -eq 'good' -and ([regex]::Matches($out, '\[FAIL\]')).Count -ne 0) {
        $problems.Add('正确配置不应有任何 FAIL')
    }

    if ($problems.Count -eq 0) {
        Write-Host ("  [ OK ] {0,-12} exit={1}  {2}" -f $c.name, $code, $c.desc) -ForegroundColor Green
    }
    else {
        $failed.Add("$($c.name): $($problems -join '; ')")
        Write-Host ("  [FAIL] {0,-12} exit={1}  {2}" -f $c.name, $code, $c.desc) -ForegroundColor Red
        $problems | ForEach-Object { Write-Host "         - $_" -ForegroundColor Red }
    }
}

# ---- 泄露断言 ----
Write-Host ''
if ($allOutput -match [regex]::Escape($PROBE_KEY)) {
    $failed.Add('JWT 密钥原文出现在输出里 —— 违反「凭据永不打印」')
    Write-Host '  [FAIL] 泄露: JWT 密钥原文出现在输出里' -ForegroundColor Red
}
else { Write-Host '  [ OK ] 泄露断言: JWT 密钥原文未出现在任何输出里' -ForegroundColor Green }

if ($allOutput -match [regex]::Escape($PROBE_PEPPER)) {
    $failed.Add('pepper 原文出现在输出里 —— 违反「凭据永不打印」')
    Write-Host '  [FAIL] 泄露: pepper 原文出现在输出里' -ForegroundColor Red
}
else { Write-Host '  [ OK ] 泄露断言: pepper 原文未出现在任何输出里' -ForegroundColor Green }

if (-not $KeepArtifacts -and (Test-Path -LiteralPath $work)) {
    & "$env:USERPROFILE\.minimax\bin\mavis-trash.cmd" -- $work *> $null
}

Write-Host ''
if ($failed.Count -gt 0) {
    Write-Host "preflight 回归测试失败: $($failed.Count) 项" -ForegroundColor Red
    exit 1
}
Write-Host "preflight 回归测试通过: $($cases.Count) 个用例 + 2 条泄露断言" -ForegroundColor Green
exit 0
