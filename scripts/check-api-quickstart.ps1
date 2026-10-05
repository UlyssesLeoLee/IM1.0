# ============================================================================
# check-api-quickstart.ps1 —— 把 docs/api/QUICKSTART.md 的端点表钉在 openapi.json 上
# ============================================================================
#
# ## 为什么需要这个门禁
#
# 接入方**只看 quickstart**, 不看 openapi.json。于是 quickstart 会腐烂: 加了端点
# 没更新文档、或改了 path 没更新文档, 都没有任何东西会红 —— 直到某个接入方照着
# 文档打过去拿到 404。
#
# 本脚本对**两个方向**都判:
#   1. quickstart 里写了但 openapi.json 没有 -> 文档在骗人
#   2. openapi.json 有了但 quickstart 没写   -> 文档漏了新能力
#
# ## 关键: 解析到 0 行必须**判失败**
#
# 最容易写坏的形态是「正则一条都没匹配到, 于是违规集合为空, 判定通过」。那等于
# 一个永远绿的检查。本脚本显式断言匹配数 > 0 且与 spec 数量相等。
#
# ## fail-closed
#
# 读不到文件 / JSON 解析失败 / 正则异常, 一律非 0 退出, 不静默跳过。

$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8

$root = Split-Path -Parent $PSScriptRoot
$quickstartPath = Join-Path $root 'docs/api/QUICKSTART.md'
$openapiPath    = Join-Path $root 'docs/api/openapi.json'

$errors = New-Object System.Collections.Generic.List[string]
function Add-Violation { param([string]$m) $errors.Add($m) }

# ---- 1. 读文件 (fail-closed) -------------------------------------------------
if (-not (Test-Path $quickstartPath)) { Add-Violation "缺少 $quickstartPath"; }
if (-not (Test-Path $openapiPath))    { Add-Violation "缺少 $openapiPath" }
if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Host "  $_" }
    Write-Host "FAIL: 读不到输入文件"
    exit 1
}

try {
    $md   = Get-Content -Raw -Encoding UTF8 $quickstartPath
    $spec = Get-Content -Raw -Encoding UTF8 $openapiPath | ConvertFrom-Json
} catch {
    Write-Host "FAIL: 读文件/解析 JSON 出错: $_"
    exit 1
}

# ---- 2. 抽 openapi 的 (method, path) -----------------------------------------
$specOps = @{}
foreach ($p in $spec.paths.PSObject.Properties) {
    foreach ($m in $p.Value.PSObject.Properties) {
        if ($m.Name -in @('get','post','put','patch','delete')) {
            $specOps["$($m.Name.ToUpper()) $($p.Name)"] = $true
        }
    }
}
if ($specOps.Count -eq 0) {
    Write-Host "FAIL: openapi.json 里一条 operation 都没有 —— paths 是不是被清空了?"
    exit 1
}

# ---- 3. 抽 quickstart 表格里的 (method, path) --------------------------------
# 表格行形如:  | 运维 | `GET` | `/healthz` | 用途 |
#            | 鉴权 | `POST` | `/v1/auth/guest` | ... |
# 刻意要求 method 与 path **都**带反引号, 避免误吃散文里的行内代码。
#
# 第一列(分组名)是**必填**的: 用 `[^|\r\n]*` 吃掉它, 而不是写成可选, 是为了让
# 表格形状变化时正则**匹配不到**从而触发上面那条 fail-closed, 而不是悄悄少读
# 几行。2026-10-06 首次跑本门禁时正是踩了这个: 正则原以为 method 在第一列,
# 于是一行都没匹配到 —— 是 fail-closed 那一段救了它, 否则会是一个永远绿的检查。
$mdOps = @{}
$rx = '(?m)^\|[^|\r\n]*\|\s*`(?<m>GET|POST|PUT|PATCH|DELETE)`\s*\|\s*`(?<p>[^`]+)`\s*\|'
foreach ($hit in [regex]::Matches($md, $rx)) {
    $mdOps["$($hit.Groups['m'].Value) $($hit.Groups['p'].Value)"] = $true
}

if ($mdOps.Count -eq 0) {
    Write-Host 'FAIL: 从 quickstart 端点表里一条都没解析出来。'
    Write-Host '      表格格式变了的话, 本门禁会**静默失去鉴别力**而不是报错 ——'
    Write-Host '      所以这里判失败。期望形如: | 运维 | `GET` | `/healthz` | 用途 |'
    exit 1
}

# ---- 4. 双向比对 -------------------------------------------------------------
foreach ($k in ($mdOps.Keys | Sort-Object)) {
    if (-not $specOps.ContainsKey($k)) {
        Add-Violation "quickstart 写了 openapi.json 没有的端点: $k"
    }
}
foreach ($k in ($specOps.Keys | Sort-Object)) {
    if (-not $mdOps.ContainsKey($k)) {
        Add-Violation "openapi.json 有但 quickstart 漏了的端点: $k"
    }
}

# ---- 5. 正文里那句「N 个 operation」也要对上 ---------------------------------
# 表格对了但正文数字没改, 读者仍会被误导。散文里写的是「## 1. 端点全表（N 个 operation）」
$countRx = '端点全表（\s*(?<n>\d+)\s*个 operation'
$cm = [regex]::Match($md, $countRx)
if (-not $cm.Success) {
    Add-Violation "quickstart 里找不到「端点全表（N 个 operation）」那一行 —— 无法校验正文里的数量"
} elseif ([int]$cm.Groups['n'].Value -ne $specOps.Count) {
    Add-Violation ("quickstart 正文说 {0} 个 operation, 实际 openapi.json 有 {1} 个" -f `
        $cm.Groups['n'].Value, $specOps.Count)
}

# ---- 6. 汇总 (先打印再退出) --------------------------------------------------
if ($errors.Count -gt 0) {
    Write-Host "check-api-quickstart: $($errors.Count) 处不一致"
    $errors | ForEach-Object { Write-Host "  $_" }
    exit 1
}

Write-Host ("OK: quickstart 端点表与 openapi.json 双向一致 ({0} 个 operation, 正文数量也相符)" -f $specOps.Count)
exit 0
