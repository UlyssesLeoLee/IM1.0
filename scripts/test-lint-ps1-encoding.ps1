# ============================================================================
# test-lint-ps1-encoding.ps1
# 用途: 变异测试 —— 证明 lint-ps1-encoding.ps1 真的会红, 且红得准。
#
# ## 为什么需要这个测试
#
# 2026-10-07 实测: lint-ps1-encoding.ps1 在**任何 worktree 内恒绿**。
# 成因是排除模式拿绝对路径去 match, 而 worktree 里 `$repoRoot` 自身
# 就是 `D:\IM1.0\.worktrees\laneA` —— 含 `\.worktrees\`, 于是每个文件的
# FullName 都命中排除条件, 扫到 0 个文件, 走「未发现 .ps1 文件」分支
# 报 OK 退出。
#
# 也就是说这个门禁**从来没有在子代理的工作树里检查过任何东西**, 而 CI
# 一直在跑它、一直是绿的。这不是「门禁不够严」, 是门禁根本没被执行。
#
# ## 判据
#
# 只跑一次正常路径**证明不了任何事** —— 旧代码(恒绿)同样会绿。所以下面
# 每一条都是「改坏 -> 必须红」, 并且要求红在**正确的位置**:
#   - 改坏 1 个文件 -> 只能点名那 1 个 (不能把 1 个问题复制成 N 条噪音)
#   - 排除吃掉全部 -> 必须 exit 2, 不能 exit 0
#   - worktree 形状的根 -> 必须扫到文件, 不能扫 0 个
#
# 用法: pwsh scripts/test-lint-ps1-encoding.ps1
# 输出: stdout + $LASTEXITCODE (0=全部断言通过, 1=有断言失败)
# 跨平台: 纯 .NET 字节操作 + pwsh 自身, Windows / Linux 行为一致。
# ============================================================================

$ErrorActionPreference = 'Stop'

$gate = Join-Path $PSScriptRoot 'lint-ps1-encoding.ps1'
if (-not (Test-Path -LiteralPath $gate)) {
    Write-Host "[ERROR] 找不到被测脚本: $gate" -ForegroundColor Red
    exit 1
}

$script:Passed = 0
$script:Failed = 0

function Assert-True {
    param([bool] $Condition, [string] $Name, [string] $Detail = '')
    if ($Condition) {
        $script:Passed++
        Write-Host ("  [ OK ] " + $Name) -ForegroundColor Green
    } else {
        $script:Failed++
        Write-Host ("  [FAIL] " + $Name) -ForegroundColor Red
        if ($Detail) { Write-Host ("         " + $Detail) -ForegroundColor Red }
    }
}

function New-Ps1Fixture {
    # 造一个 .ps1: $withBom 决定有没有 EF BB BF, $nonAscii 决定有没有 >=0x80 字节
    param([string] $Path, [bool] $withBom, [bool] $nonAscii)
    $dir = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $dir)) {
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
    }
    $text = if ($nonAscii) {
        '# 中文字符串, 用来触发 GBK 误判' + [Environment]::NewLine + '$v = 1' + [Environment]::NewLine
    } else {
        '# ascii only' + [Environment]::NewLine + '$v = 1' + [Environment]::NewLine
    }
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($text)
    if ($withBom) {
        $pre = New-Object byte[] 3
        $pre[0] = 0xEF; $pre[1] = 0xBB; $pre[2] = 0xBF
        $all = New-Object byte[] ($bytes.Length + 3)
        [Array]::Copy($pre, 0, $all, 0, 3)
        [Array]::Copy($bytes, 0, $all, 3, $bytes.Length)
        [System.IO.File]::WriteAllBytes($Path, $all)
    } else {
        [System.IO.File]::WriteAllBytes($Path, $bytes)
    }
}

function Invoke-Gate {
    # 跑一次门禁, 返回 @{ Code; Out }
    #
    # $ExtraArgs 必须是**数组**: 传单个字符串时 `-ExcludeSegments scripts` 会作为
    # 一个 argv 元素过去, 参数根本绑不上 —— 门禁照常用默认排除名单跑, 于是 T3
    # 拿到的是 b.ps1 的 exit 1 而不是排除失效的 exit 2。
    # 症状是「断言红, 但门禁看起来没做我让它做的事」, 不是门禁的错。
    param([string] $RepoRoot, [string[]] $ExtraArgs = @())
    $out = & pwsh -NoProfile -File $gate -RepoRoot $RepoRoot @ExtraArgs 2>&1 | Out-String
    return @{ Code = $LASTEXITCODE; Out = $out }
}

# 每个用例一个独立目录, 避免互相污染; 用 .worktrees 开头的根来复现原 bug。
$tmpBase = Join-Path ([System.IO.Path]::GetTempPath()) ('im10-lint-gate-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))

try {
    Write-Host "=== lint-ps1-encoding 变异测试 ==="
    Write-Host ("被测: " + $gate)
    Write-Host ("临时根: " + $tmpBase)
    Write-Host ''

    # ---------------------------------------------------------------------
    # T1 对照组: 真实仓库当前是合规的, 门禁必须 exit 0
    # ---------------------------------------------------------------------
    $repoRoot = Split-Path -Parent $PSScriptRoot
    $r1 = Invoke-Gate -RepoRoot $repoRoot
    Assert-True ($r1.Code -eq 0) 'T1 真实仓库全合规 -> exit 0' ("exit=" + $r1.Code)

    # ---------------------------------------------------------------------
    # T2 覆盖面: **没有任何被 git 跟踪的 .ps1 被排除模式吃掉**
    # 排除名单是手写的, 往里加一个 'scripts' 就能让门禁扫不到任何文件。
    # T3 只证明它会 exit 2; 这一条证明「正常情况下真的扫到了仓库的脚本」。
    # ---------------------------------------------------------------------
    $excl = @('.git', 'target', 'node_modules', '.worktrees', 'dist')
    $tracked = @(git -C $repoRoot ls-files '*.ps1' 2>$null)
    $eaten = @()
    foreach ($t in $tracked) {
        $segs = @(($t -split '[\\/]')[0..([Math]::Max(0, ($t -split '[\\/]').Count - 2))])
        if (@($segs | Where-Object { $excl -contains $_ }).Count -gt 0) { $eaten += $t }
    }
    Assert-True ($tracked.Count -gt 0) 'T2a 仓库里存在被跟踪的 .ps1 (对照组非空)' ("tracked=" + $tracked.Count)
    Assert-True ($eaten.Count -eq 0) 'T2b 没有 tracked .ps1 被排除模式吃掉' ("被吃掉: " + ($eaten -join ', '))

    # ---------------------------------------------------------------------
    # T3 fail-closed: 排除模式吃掉全部时必须 exit 2, **不能 exit 0**
    # 这正是 worktree bug 的形态: 扫 0 个文件却报「无需检查」。
    # ---------------------------------------------------------------------
    $wtRoot = Join-Path $tmpBase '.worktrees\_wt'
    New-Ps1Fixture -Path (Join-Path $wtRoot 'scripts\a.ps1') -withBom $true  -nonAscii $true
    New-Ps1Fixture -Path (Join-Path $wtRoot 'scripts\b.ps1') -withBom $false -nonAscii $true
    $r3 = Invoke-Gate -RepoRoot $wtRoot -ExtraArgs @('-ExcludeSegments', 'scripts')
    Assert-True ($r3.Code -eq 2) 'T3 排除吃掉全部 -> exit 2 (fail-closed, 不是 exit 0)' ("exit=" + $r3.Code)

    # ---------------------------------------------------------------------
    # T4 worktree 形状的根: 旧逻辑这里扫 0 个并 exit 0, 新逻辑必须扫到并报错
    # 这是 2026-10-07 那个 bug 的直接回归测试。
    # ---------------------------------------------------------------------
    $r4 = Invoke-Gate -RepoRoot $wtRoot
    $failLines4 = @([regex]::Matches($r4.Out, '\[FAIL\]\s+NO-BOM') )
    Assert-True ($r4.Code -eq 1) 'T4 worktree 形状的根里存在违规文件 -> exit 1' ("exit=" + $r4.Code)
    Assert-True ($failLines4.Count -eq 1) 'T4b 只点名违规的那 1 个文件, 不是 0 个也不是全部' ("NO-BOM 命中=" + $failLines4.Count)
    Assert-True ($r4.Out -match 'b\.ps1') 'T4c 点名的是 b.ps1 (违规那个)'
    Assert-True ($r4.Out -match 'a\.ps1') 'T4d 合规的 a.ps1 也被扫到并报 OK (证明不是扫 0 个)'

    # ---------------------------------------------------------------------
    # T5 点名唯一性: 3 个文件只坏 1 个 -> 只能出现 1 条 NO-BOM
    # 反例形态: 一个问题被复制成 N 条噪音, 真正的问题被埋掉。
    # ---------------------------------------------------------------------
    $uRoot = Join-Path $tmpBase 'unique'
    New-Ps1Fixture -Path (Join-Path $uRoot 'scripts\p1.ps1') -withBom $true  -nonAscii $true
    New-Ps1Fixture -Path (Join-Path $uRoot 'scripts\p2.ps1') -withBom $true  -nonAscii $true
    New-Ps1Fixture -Path (Join-Path $uRoot 'scripts\p3.ps1') -withBom $false -nonAscii $true
    $r5 = Invoke-Gate -RepoRoot $uRoot
    $failLines5 = @([regex]::Matches($r5.Out, '\[FAIL\]\s+NO-BOM') )
    Assert-True ($r5.Code -eq 1) 'T5 3 个文件坏 1 个 -> exit 1' ("exit=" + $r5.Code)
    Assert-True ($failLines5.Count -eq 1) 'T5b 只出现 1 条 NO-BOM (问题不被复制成 N 条)' ("NO-BOM 命中=" + $failLines5.Count)
    Assert-True ($r5.Out -match 'p3\.ps1') 'T5c 点名的是 p3.ps1 (被改坏的那个)'

    # ---------------------------------------------------------------------
    # T6 纯 ASCII 且无 BOM **不算违规**
    # 这是仓库里 check-openapi.ps1 的形状。规则是「含非 ASCII 才必须有 BOM」,
    # 不是「所有文件都必须有 BOM」。判重了会把好文件判坏。
    # ---------------------------------------------------------------------
    $aRoot = Join-Path $tmpBase 'ascii'
    New-Ps1Fixture -Path (Join-Path $aRoot 'scripts\ascii.ps1')  -withBom $false -nonAscii $false
    New-Ps1Fixture -Path (Join-Path $aRoot 'scripts\bom.ps1')    -withBom $true  -nonAscii $false
    $r6 = Invoke-Gate -RepoRoot $aRoot
    Assert-True ($r6.Code -eq 0) 'T6 纯 ASCII 无 BOM 不算违规 -> exit 0' ("exit=" + $r6.Code)

    # ---------------------------------------------------------------------
    # T7 真的没有 .ps1 时 -> exit 0, 且措辞与「被排除光」不同
    # 这两条都是 exit 0, 但只有一条是安全的。混为一谈就是恒绿的门禁。
    # ---------------------------------------------------------------------
    $emptyRoot = Join-Path $tmpBase 'empty'
    New-Item -ItemType Directory -Force -Path $emptyRoot | Out-Null
    $r7 = Invoke-Gate -RepoRoot $emptyRoot
    Assert-True ($r7.Code -eq 0) 'T7 空仓库 -> exit 0' ("exit=" + $r7.Code)
    Assert-True ($r7.Out -match '确实没有') 'T7b 空仓库的措辞是「确实没有」, 与 T3 的排除失效可区分'
    Assert-True (-not ($r3.Out -match '确实没有')) 'T7c 排除失效时不许说「确实没有」(fail-closed 措辞)'
} finally {
    if (Test-Path -LiteralPath $tmpBase) {
        # 脚本内部删自己的临时目录; 与工作区产物无关, 故不走 mavis-trash。
        Remove-Item -LiteralPath $tmpBase -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Write-Host ''
Write-Host ("通过 " + $script:Passed + " / 失败 " + $script:Failed)
if ($script:Failed -gt 0) {
    Write-Host '[FAIL] lint-ps1-encoding 的变异测试没全过 —— 不要相信它现在是绿的。' -ForegroundColor Red
    exit 1
}
Write-Host '[OK] 变异测试全过: 门禁在该红的时候红, 且只红在该红的那一个上。' -ForegroundColor Green
exit 0