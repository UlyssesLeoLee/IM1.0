#Requires -Version 7.0
<#
.SYNOPSIS
    build-package.ps1 的平台映射回归测试 —— 纯函数 Get-PackageTarget。

.DESCRIPTION
    为什么要有这个：`build-package.ps1` 里「当前平台 → (包名后缀, 二进制扩展名)」
    此前是两行写死的 `-win-x64` 与 `"$b.exe"`。这类写死不会让任何东西变红，
    它只是让 Linux/macOS 的接入方**拿不到任何二进制分发包**，而 CI 全绿。

    把它抽成纯函数后，「Linux 上会得到什么」这个问题就可以在**一台 Windows
    机器**上回答 —— 而这正是它必须被单测钉住的理由：本机永远只能产出
    win-x64 那一格，另外两格没有别的机制保证不被改坏。

    断言分四组：
      1. 对照组：三个平台 × 必备字段（任务书点名的三行：win-x64+.exe /
         linux-x64+空 / darwin-*+空）
      2. 副产物：RustTarget / IsWindows 必须与 Suffix 自洽（否则 BUILD-INFO
         里会记下与包名矛盾的 triple）
      3. fail-closed：未知 OS / 未知架构必须**抛异常**，不得编一个包名
      4. 装置守卫：dot-source 必须只取函数、不跑打包流程（否则「测纯函数」
         这个动作本身会去删 `dist/` 重建）

    判别力已用变异测试证明（改坏后本脚本 exit 1，改回后 exit 0）：
      - linux 的 Ext 写成 '.exe'
      - linux 的 Slug 写成 'win'
      - macOS 的 Ext 写成 '.exe'
      - Arm64 的 Suffix 仍写死 'x64'
      - 删掉 default 抛异常分支（未知平台静默回落到 win）
      - 删掉 dot-source 闸（导入脚本开始跑打包流程）

.EXAMPLE
    pwsh -NoProfile -File scripts/test-build-package.ps1
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

$repo  = Split-Path -Parent $PSScriptRoot
$build = Join-Path $repo 'scripts/build-package.ps1'
if (-not (Test-Path -LiteralPath $build)) { throw "找不到 $build" }

$script:pass = 0
$script:fail = New-Object System.Collections.Generic.List[string]

function Assert-True {
    param([string] $Name, [bool] $Condition, [string] $Detail = '')
    if ($Condition) {
        $script:pass++
        Write-Host ("  [ OK ] {0}" -f $Name) -ForegroundColor DarkGray
    } else {
        $script:fail.Add("$Name$Detail")
        Write-Host ("  [FAIL] {0}{1}" -f $Name, $Detail) -ForegroundColor Red
    }
}

function Assert-Equal {
    param([string] $Name, $Expected, $Actual)
    Assert-True -Name $Name -Condition ($Expected -ceq $Actual) `
        -Detail ("  期望 [{0}] 实际 [{1}]" -f $Expected, $Actual)
}

Write-Host '=== Get-PackageTarget 平台映射回归测试 ==='
Write-Host ''

# ---- 装置守卫: dot-source 只取函数, 不得触发打包流程 ----------------------------
# 先记下 dist/ 的存在性。build-package 删掉并重建的是**整个 dist/<包名> 目录**,
# 若 dot-source 真的跑起来, 这里就会凭空多出一个目录 —— 那说明闸门失效了。
$distBefore = Test-Path -LiteralPath (Join-Path $repo 'dist')
. $build
Assert-True -Name '装置守卫: dot-source 不创建 dist/' `
    -Condition ((Test-Path -LiteralPath (Join-Path $repo 'dist')) -eq $distBefore) `
    -Detail '  dot-source 跑起了打包流程'
Assert-True -Name '装置守卫: dot-source 后 Get-PackageTarget 可用' `
    -Condition ((Get-Command Get-PackageTarget -ErrorAction SilentlyContinue) -ne $null) `
    -Detail '  函数没被导出'

# ---- 1. 对照组: 任务书点名的三行 ----------------------------------------------
Write-Host ''
Write-Host '--- 1. 三平台对照 ---'

$win = Get-PackageTarget -OSPlatform 'Windows' -Architecture 'X64'
Assert-Equal 'Windows/X64  Suffix'    'win-x64'   $win.Suffix
Assert-Equal 'Windows/X64  Extension' '.exe'      $win.Extension
Assert-True  'Windows/X64  IsWindows' $win.IsWindows

$lin = Get-PackageTarget -OSPlatform 'Linux' -Architecture 'X64'
Assert-Equal 'Linux/X64    Suffix'    'linux-x64' $lin.Suffix
Assert-Equal 'Linux/X64    Extension' ''          $lin.Extension
Assert-True  'Linux/X64    IsWindows 为假' (-not $lin.IsWindows)

$mac = Get-PackageTarget -OSPlatform 'macOS' -Architecture 'X64'
Assert-Equal 'macOS/X64    Suffix'    'darwin-x64' $mac.Suffix
Assert-Equal 'macOS/X64    Extension' ''            $mac.Extension
Assert-True  'macOS/X64    IsWindows 为假' (-not $mac.IsWindows)

# ---- 2. 副产物自洽 ------------------------------------------------------------
# RustTarget 会写进 BUILD-INFO.txt。若它与 Suffix 矛盾, 报障时 operator 拿到的
# 两条元信息会互相打架, 且没有任何东西会红。
Write-Host ''
Write-Host '--- 2. RustTarget / IsWindows 与 Suffix 自洽 ---'
Assert-Equal 'Windows/X64  RustTarget' 'x86_64-pc-windows-msvc' $win.RustTarget
Assert-Equal 'Linux/X64    RustTarget' 'x86_64-unknown-linux-gnu' $lin.RustTarget
Assert-Equal 'macOS/X64    RustTarget' 'x86_64-apple-darwin'      $mac.RustTarget
Assert-Equal 'macOS/Arm64  Suffix'     'darwin-arm64' (Get-PackageTarget -OSPlatform 'macOS' -Architecture 'Arm64').Suffix
Assert-Equal 'macOS/Arm64  RustTarget' 'aarch64-apple-darwin' (Get-PackageTarget -OSPlatform 'macOS' -Architecture 'Arm64').RustTarget
Assert-Equal 'Windows/Arm64 Suffix'    'win-arm64'   (Get-PackageTarget -OSPlatform 'Windows' -Architecture 'Arm64').Suffix
Assert-Equal 'Windows/Arm64 Extension' '.exe'        (Get-PackageTarget -OSPlatform 'Windows' -Architecture 'Arm64').Extension
Assert-Equal 'Linux/Arm64  Suffix'     'linux-arm64' (Get-PackageTarget -OSPlatform 'Linux' -Architecture 'Arm64').Suffix

# 形状守卫: 后缀必须始终是 <os>-<arch>, 否则包名约定被改坏而上面几条还可能通过
# (例如把 Suffix 改成 'im1.0-linux' 之类)。
$shapeOk = $true
foreach ($t in @(
    (Get-PackageTarget -OSPlatform 'Windows' -Architecture 'X64'),
    (Get-PackageTarget -OSPlatform 'Linux'   -Architecture 'X64'),
    (Get-PackageTarget -OSPlatform 'macOS'   -Architecture 'X64'),
    (Get-PackageTarget -OSPlatform 'macOS'   -Architecture 'Arm64')
)) {
    if ($t.Suffix -cnotmatch '^(win|linux|darwin)-(x64|arm64)$') { $shapeOk = $false }
}
Assert-True 'Suffix 形状守卫 (win|linux|darwin)-(x64|arm64)' $shapeOk

# 大小写: RuntimeInformation 实际吐的是 'macOS'。但调用点也可能拿到 'macos' /
# 'MACOS' —— 与其赌调用点一定规范, 不如让函数本身大小写不敏感。
Assert-Equal '大小写不敏感: macos 仍解析为 darwin' 'darwin-x64' (Get-PackageTarget -OSPlatform 'macos' -Architecture 'X64').Suffix
Assert-Equal '大小写不敏感: x64 仍解析为 x64'       'win-x64'   (Get-PackageTarget -OSPlatform 'Windows' -Architecture 'x64').Suffix

# ---- 3. fail-closed: 未知平台 / 架构必须抛 ------------------------------------
# 这是本组断言里最重要的一条: 一个「不认识就回落到 win」的映射表不会让任何
# 其它用例变红, 但它会让 FreeBSD 上的产物顶着 win-x64 的名字发出去。
Write-Host ''
Write-Host '--- 3. 未知平台/架构必须抛异常 ---'

$threwOs = $false
$osMsg = ''
try { $null = Get-PackageTarget -OSPlatform 'FreeBSD' -Architecture 'X64' }
catch { $threwOs = $true; $osMsg = $_.Exception.Message }
Assert-True '未知 OS 抛异常 (FreeBSD)' $threwOs '  静默回落了'
Assert-True '未知 OS 的异常消息点出该 OS' ($osMsg -like '*FreeBSD*') "  消息: $osMsg"

$threwArch = $false
$archMsg = ''
try { $null = Get-PackageTarget -OSPlatform 'Linux' -Architecture 'RiscV64' }
catch { $threwArch = $true; $archMsg = $_.Exception.Message }
Assert-True '未知架构抛异常 (RiscV64)' $threwArch '  静默回落了'
Assert-True '未知架构的异常消息点出该架构' ($archMsg -like '*RiscV64*') "  消息: $archMsg"

# ---- 4. 真实平台探测 → 契约名 ----
# 这一组是为一个**真实踩到的 bug** 写的: 接线时用了
# `[RuntimeInformation]::OSPlatform`, 而在 pwsh 7.6.6(.NET 10) 上那个静态属性
# 已经不存在, 脚本在第一步就抛「找不到属性 OSPlatform」。
#
# 纯函数那 26 条断言**抓不到**这个 bug —— 它们从不接触真实运行环境。所以这里断言
# 「探测出来的名字必须能被 Get-PackageTarget 接受」: 探测与映射之间一旦对不上
# (大小写、改名、新 .NET 换了 API 面), 打包就在第一行炸掉, 而那种失败与
# 「平台没做」看起来完全一样。
Write-Host ''
Write-Host '--- 4. 当前平台探测与映射契约一致 ---'

$detected = $null
$detectThrew = $false
try { $detected = Get-CurrentOSPlatform } catch { $detectThrew = $true }
Assert-True 'Get-CurrentOSPlatform 不抛异常' (-not $detectThrew)
if (-not $detectThrew) {
    Write-Host ("       (本机探测结果: {0})" -f $detected)
    Assert-True '探测结果属于契约名 {Windows,Linux,macOS}' ($detected -in @('Windows', 'Linux', 'macOS')) `
        "  实际: [$detected]"
    $probeOk = $true
    try { $null = Get-PackageTarget -OSPlatform $detected -Architecture 'X64' } catch { $probeOk = $false }
    Assert-True '探测结果能被 Get-PackageTarget 接受' $probeOk
}

# ---- 汇总 ---------------------------------------------------------------------
Write-Host ''
Write-Host ("通过 {0} 条, 失败 {1} 条" -f $script:pass, $script:fail.Count)
if ($script:fail.Count -gt 0) {
    Write-Host ''
    foreach ($f in $script:fail) { Write-Host "  - $f" -ForegroundColor Red }
    Write-Host 'FAIL: 平台映射回归测试未通过' -ForegroundColor Red
    exit 1
}
Write-Host '[OK] 平台映射回归测试全部通过' -ForegroundColor Green
exit 0
