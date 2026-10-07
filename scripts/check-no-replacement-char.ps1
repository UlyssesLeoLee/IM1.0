# ============================================================================
# check-no-replacement-char.ps1
# 用途: 仓库内**被 git 跟踪**的文件不得含 U+FFFD (REPLACEMENT CHARACTER)。
#
# ## 为什么需要这个检查
#
# 2026-10-07 实测: 全仓 447 个跟踪文件里有 3 个含 U+FFFD, 且**都是出生即坏** ——
# `git cat-file` 逐个版本比对确认乱码在它们被引入的那个提交里就存在, 历史上
# 从来没有过干净版本。成因是提交时编码被换过一次(UTF-8 三字节汉字被打成
# `EF BF BD`), 不是后来某个 diff 改坏的。
#
# 后果不限于「看着难受」:
#   - `packaging/template/env.example` 与 `packaging/template/scripts/preflight.ps1`
#     **直接进分发包**。也就是说已交付给接入方的安装包里, 环境变量模板和
#     preflight 脚本都带着乱码 —— 而这两个文件恰好是接入方**第一眼**要读的
#     「怎么配」与「配对没」的说明。
#   - U+FFFD 本身是**合法 UTF-8**, 所以「文件是合法 UTF-8」这类检查抓不到它。
#     编码检查(python / UTF-8 解码)全绿, 文件里却有一坨乱码。
#
# ## 为什么扫的是「被 git 跟踪的文件」而不是目录树
#
# 目录树会把 target/ node_modules/ dist/ 里的构建产物也扫进来, 既慢又会
# 因为本地产物不同而让门禁结果随机器而变。`git ls-files` 让被测对象与「仓库
# 里到底有什么」严格一致 —— 这正是本仓库其它 ps1 门禁踩过的坑
# (见 lint-ps1-encoding 的排除模式曾把整个 worktree 排掉)。
#
# ## 判定
#
#   文件含字节序列 EF BF BD  -> 违规
#   文件不是合法 UTF-8       -> 跳过(二进制), 但**计数并打印**
#
# 跳过必须可见: 静默跳过会让「扫描范围悄悄变窄」和「扫到了且没问题」长得
# 一模一样 —— 这正是本仓已吃过一次的亏(某个扫描器只认一种形态就报 OK)。
#
# 用法: pwsh scripts/check-no-replacement-char.ps1
# 输出: stdout + $LASTEXITCODE (0=干净, 1=有违规, 2=扫描器自身失效)
# 跨平台: 纯字节操作 + git, Windows / Linux 行为一致。
# ============================================================================

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $repoRoot) {
    Write-Host '[ERROR] 无法定位仓库根目录 ($PSScriptRoot 为空)' -ForegroundColor Red
    exit 1
}

Write-Host '=== U+FFFD (替换字符) 检查 ==='
Write-Host ("仓库根: " + $repoRoot)
Write-Host ''

# --- 列出被跟踪的文件; git 失败必须响, 不能当成「0 个文件 = 干净」 ---
$raw = & git -C $repoRoot ls-files -z 2>&1
$gitExit = $LASTEXITCODE
if ($gitExit -ne 0) {
    Write-Host '[FAIL] `git ls-files` 执行失败, 无法确定被测对象。' -ForegroundColor Red
    Write-Host ("       exit=" + $gitExit + " 输出: " + ($raw | Out-String)) -ForegroundColor Red
    Write-Host '       门禁**失效**, 不等于通过。' -ForegroundColor Red
    exit 2
}

# NUL 分隔; PowerShell 管道按行拆, 故先转成字符串再按 [char]0 切
$blob = ($raw | Out-String)
$files = @($blob -split [char]0 | Where-Object { $_ -ne '' })

if ($files.Count -eq 0) {
    Write-Host '[FAIL] `git ls-files` 返回 0 个文件 —— 扫描器失效, 不是仓库干净。' -ForegroundColor Red
    exit 2
}

$utf8Strict = New-Object System.Text.UTF8Encoding($false, $true)  # throwOnInvalidBytes = true
$replacement = [byte[]](0xEF, 0xBB, 0xBD)

$violations = New-Object System.Collections.Generic.List[string]
$scanned = 0
$skipped = 0

foreach ($rel in $files) {
    $abs = Join-Path $repoRoot $rel
    if (-not (Test-Path -LiteralPath $abs -PathType Leaf)) { continue }

    $bytes = [System.IO.File]::ReadAllBytes($abs)

    # 先判是不是合法 UTF-8; 非法的一律跳过(二进制), 但计数要打印
    try {
        $null = $utf8Strict.GetString($bytes)
    } catch {
        $skipped++
        continue
    }
    $scanned++

    for ($i = 0; $i -le $bytes.Length - 3; $i++) {
        if ($bytes[$i] -eq 0xEF -and $bytes[$i + 1] -eq 0xBB -and $bytes[$i + 2] -eq 0xBD) {
            $violations.Add($rel)
            Write-Host ("  [FAIL] " + $rel) -ForegroundColor Red
            $line = ([System.Text.Encoding]::UTF8.GetString($bytes) -split "`r?`n") |
                Where-Object { $_.Contains([char]0xFFFD) } | Select-Object -First 1
            if ($line) {
                Write-Host ("         " + $line.Trim()) -ForegroundColor Red
            }
            break
        }
    }
}

Write-Host ''
Write-Host ("已扫描 " + $scanned + " 个文本文件(合法 UTF-8)")
if ($skipped -gt 0) {
    Write-Host ("跳过 " + $skipped + " 个非 UTF-8 文件(二进制, 不在本检查范围)") -ForegroundColor DarkGray
}

if ($violations.Count -gt 0) {
    Write-Host ''
    Write-Host ("[FAIL] " + $violations.Count + " 个文件含 U+FFFD 替换字符。") -ForegroundColor Red
    Write-Host '       U+FFFD 本身是合法 UTF-8, 所以编码检查抓不到它; 它意味着某个' -ForegroundColor Red
    Write-Host '       非 ASCII 字符在写入时被换过编码, 原文已不可考。' -ForegroundColor Red
    Write-Host ''
    Write-Host '       修法: 若原文可考(用 git log -S 找到引入提交, 逐版本 cat-file' -ForegroundColor Red
    Write-Host '       比对), 按上下文恢复; 若确实不可考, **按上下文重写该句**并在' -ForegroundColor Red
    Write-Host '       docs/gap-ledger.md 记一笔, 不要留乱码, 也不要在本脚本里加例外。' -ForegroundColor Red
    exit 1
}

Write-Host ''
Write-Host '[OK] 全部干净: 被跟踪的文本文件中没有 U+FFFD' -ForegroundColor Green
exit 0