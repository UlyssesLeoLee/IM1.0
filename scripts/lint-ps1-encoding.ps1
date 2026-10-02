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
# 输出: stdout + $LASTEXITCODE (0=全部合规, 1=存在违规文件)
# ============================================================================

$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $repoRoot) {
    Write-Host '[ERROR] 无法定位仓库根目录 ($PSScriptRoot 为空)' -ForegroundColor Red
    exit 1
}

Write-Host "=== ps1 编码合规检查 (要求: 含非 ASCII 字节的文件必须带 UTF-8 BOM) ==="
Write-Host ("仓库根: " + $repoRoot)
Write-Host ''

# 排除构建产物与工作树, 避免把 target 里的拷贝也扫进来
$excludePattern = '\\(\.git|target|node_modules|\.worktrees)\\'

$files = Get-ChildItem -Path $repoRoot -Recurse -Filter '*.ps1' -File -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -notmatch $excludePattern }

if (-not $files) {
    Write-Host '[OK] 未发现 .ps1 文件, 无需检查' -ForegroundColor Green
    exit 0
}

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

    $rel = $f.FullName.Substring($repoRoot.Length + 1)
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
