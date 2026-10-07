# ============================================================================
# lint-ps1-encoding.ps1
# 用途: 防回归检查 —— 仓库内所有 .ps1 必须是 UTF-8 with BOM。
#
# 为什么需要这个检查 (per 296927e 的实测):
#   Windows PowerShell 5.1 读取无 BOM 的 .ps1 时, 按系统 ANSI 代码页
#   (简体中文 Windows = GBK/CP936) 解码。GBK 双字节的**第二字节**合法范围
#   是 0x40-0x7E, 包含 ASCII 符号位(引号、括号等)。因此一个中文字符的
#   UTF-8 字节会被误判为 GBK 前导字节, 并把紧随其后的引号当成第二字节吞掉,
#   导致字符串提前终止。
#
#   实测后果: 含中文的脚本在 5.1 下产生 6~26 个解析错误, 报 MissingEndCurlyBrace /
#   "字符串缺少终止符", 错误信息完全指不到真实原因; 开发者敲
#   `powershell scripts\<name>.ps1` 直接崩。修法是加 UTF-8 BOM。
#
# 判定规则:
#   文件含任意非 ASCII 字节(>= 0x80)  且  缺少 UTF-8 BOM  →  违规
#   纯 ASCII 文件缺 BOM 不算违规 (ANSI 与 UTF-8 对纯 ASCII 解码一致)。
#
# 跨平台: 纯字节操作, 不依赖任何文本解码, Windows / Linux 行为一致。
#         需在有 pwsh 7 的环境下运行 (GitHub Actions ubuntu runner 自带)。
#
# 用法: pwsh scripts/lint-ps1-encoding.ps1
# 输出: stdout + $LASTEXITCODE (0=全部合规, 1=存在违规文件, 2=门禁自身失效)
#
# 参数仅供 scripts/test-lint-ps1-encoding.ps1 的变异测试使用, 正常使用不要传。
# ============================================================================

param(
    [string]   $RepoRoot,
    [string[]] $ExcludeSegments
)

$ErrorActionPreference = 'Stop'

if (-not $RepoRoot) {
    $RepoRoot = Split-Path -Parent $PSScriptRoot
}
if (-not $RepoRoot) {
    Write-Host '[ERROR] 无法定位仓库根目录 ($PSScriptRoot 为空)' -ForegroundColor Red
    exit 1
}

Write-Host "=== ps1 编码合规检查 (要求: 含非 ASCII 字节的文件必须带 UTF-8 BOM) ==="
Write-Host ("仓库根: " + $RepoRoot)
Write-Host ''

# 排除构建产物与工作树, 避免把 target 里的拷贝也扫进来。
#
# ## 关键: 必须按**相对路径**排除, 不能拿绝对路径去 match
#
# 在 worktree 里 `$repoRoot` 本身就是 `D:\IM1.0\.worktrees\laneA` ——
# 它**含有** `\.worktrees\`。于是每个文件的 FullName 都命中下面的模式,
# 结果是扫到 **0 个文件**并报 `[OK] 未发现 .ps1 文件`。
#
# 也就是说: **这个门禁在任何 worktree 内恒绿**, 扫不扫都一样绿。
# 「写了守卫但它永远不跑」比没有守卫更坏 —— 后者让人知道缺什么。
# (子代理 lane/crossplat-pkg 报了这个, 当时手工验过两个 .ps1 都带 BOM。)
#
# `dist` 是 build-package.ps1 的产物目录(.gitignore:69), 里面的 .ps1 是
# packaging/template/scripts/ 的**拷贝**。扫它等于把构建产物当被测对象: 源
# 已经单独扫过, 产物只是重复计数。真正要守的是模板, 不是它的复印件。
if (-not $ExcludeSegments -or $ExcludeSegments.Count -eq 0) {
    $ExcludeSegments = @('.git', 'target', 'node_modules', '.worktrees', 'dist')
}

function Test-Excluded {
    param([string] $AbsolutePath)
    $rel = [System.IO.Path]::GetRelativePath($RepoRoot, $AbsolutePath)
    $parts = $rel -split '[\\/]'
    # 末段是文件名, 不是目录, 故只看父目录段
    foreach ($p in $parts[0..([Math]::Max(0, $parts.Count - 2))]) {
        if ($ExcludeSegments -contains $p) { return $true }
    }
    return $false
}

$allPs1 = @(Get-ChildItem -Path $RepoRoot -Recurse -Filter '*.ps1' -File -ErrorAction SilentlyContinue)
$files = @($allPs1 | Where-Object { -not (Test-Excluded $_.FullName) })

if ($files.Count -eq 0) {
    # 0 命中**不自动等于**「无需检查」—— 那正是排除模式吃掉整个仓库时的表现。
    # 判别: 不带排除再数一遍。真的 0 个 -> 确实没有; 有却被排除光 -> 门禁坏了。
    if ($allPs1.Count -eq 0) {
        Write-Host '[OK] 仓库里确实没有 .ps1 文件, 无需检查' -ForegroundColor Green
        exit 0
    }
    Write-Host "[FAIL] 排除模式吃掉了全部 $($allPs1.Count) 个 .ps1 文件。" -ForegroundColor Red
    Write-Host "       在 worktree 内会发生这个: \$repoRoot 自身含 \.worktrees\, 使每个文件的绝对路径都命中排除条件。" -ForegroundColor Red
    Write-Host '       结果是一个在任何 worktree 里都恒绿的门禁。已按相对路径排除, 请复核本脚本。' -ForegroundColor Red
    exit 2
}

# 被排除掉的数量必须**打出来**。上面的 exit 2 只拦得住「全被吞掉」;
# 「只吞掉一部分」照样扫得到文件、照样报 OK, 在 CI 日志里跟正常通过长得
# 一模一样。数字摆在日志里, 部分吞并才看得见。
$excludedCount = $allPs1.Count - $files.Count
if ($excludedCount -gt 0) {
    Write-Host ("排除 " + $excludedCount + " 个构建产物/工作树中的 .ps1 (非仓库内容, 不在检查范围内)") -ForegroundColor DarkGray
}
Write-Host ''

$violations = New-Object System.Collections.Generic.List[string]
$checked = 0

foreach ($f in $files) {
    $bytes = [System.IO.File]::ReadAllBytes($f.FullName)

    # 空文件直接跳过
    if ($bytes.Length -eq 0) {
        continue
    }
    $checked++

    $hasBom = ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF)

    # 快速判定是否含非 ASCII 字节; 找到即可退出
    $hasNonAscii = $false
    for ($i = 0; $i -lt $bytes.Length; $i++) {
        if ($bytes[$i] -ge 0x80) {
            $hasNonAscii = $true
            break
        }
    }

    $rel = $f.FullName.Substring($RepoRoot.Length + 1)
    $flag = if ($hasBom) { 'BOM   ' } else { 'NO-BOM' }
    $enc = if ($hasNonAscii) { 'non-ascii' } else { 'ascii-only' }

    if ($hasNonAscii -and -not $hasBom) {
        $violations.Add($rel)
        Write-Host ("  [FAIL] {0}  {1}  {2}" -f $flag, $enc, $rel) -ForegroundColor Red
    } else {
        Write-Host ("  [ OK ] {0}  {1}  {2}" -f $flag, $enc, $rel) -ForegroundColor DarkGray
    }
}

Write-Host ''
Write-Host ("已检查 " + $checked + " 个 .ps1 文件")

if ($violations.Count -gt 0) {
    Write-Host ("[FAIL] " + $violations.Count + " 个含中文的 .ps1 缺少 UTF-8 BOM,") -ForegroundColor Red
    Write-Host '       它们在 Windows PowerShell 5.1 下会 ParserError 直接崩。' -ForegroundColor Red
    Write-Host ''
    Write-Host '       违规文件:' -ForegroundColor Red
    foreach ($v in $violations) {
        Write-Host ('         - ' + $v) -ForegroundColor Red
    }
    Write-Host ''
    Write-Host '修法: 在支持 UTF-8 的编辑器里对该文件"另存为 UTF-8 with BOM",' -ForegroundColor Yellow
    Write-Host '      或字节级插入 EF BB BF 三字节前缀(不影响 CRLF 与内容)。' -ForegroundColor Yellow
    exit 1
}

Write-Host '[OK] 全部合规: 含非 ASCII 的 .ps1 均带 UTF-8 BOM' -ForegroundColor Green
exit 0
