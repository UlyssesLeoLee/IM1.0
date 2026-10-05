#Requires -Version 7.0
<#
.SYNOPSIS
    组装 im1.0 分发包（安装包）并生成 SHA256SUMS。

.DESCRIPTION
    为什么要**脚本化**而不是手工 zip：

    手工打的包无法回答「这个包是从哪个 commit 出的、里面装的是哪几个
    二进制」。所以本脚本把版本号、commit、构建时间、三个二进制的 SHA256
    一并写进包内的 BUILD-INFO.txt，operator 报障时贴这一个文件就够。

    包里装什么（以及**不**装什么）：

      bin/        im-gateway / im-migrate / jobctl（release 产物）
      config/     env.example（配置契约模板）
      scripts/    install / preflight / start-gateway  ← **仅 Windows 包**
      docs/       QUICKSTART / openapi.json / asyncapi.json
      INSTALL.md  安装说明（通篇按 Windows 写）
      PLATFORM.md 平台说明  ← **仅非 Windows 包**
      LICENSE     Apache-2.0

      非 Windows 包**不带** scripts/*.ps1：这三个文件内部写死了
      `bin\im-gateway.exe` / `bin\jobctl.exe`，在只有 `bin/im-gateway` 的包里
      必然找不到目标，而它们不是打包脚本能改的文件。发一份跑不起来的安装脚本
      比不发更糟，故改为在 PLATFORM.md 里给出等价命令。

      **不含** migrations/ —— `sqlx::migrate!` 已把 SQL 编译进 im-migrate，
      带上目录反而会让人以为可以从目录跑迁移，出现「目录里的 SQL 比
      二进制旧」这种最难排查的状态。

      **不含** .env / 任何密钥。

    平台：脚本按**当前运行平台**出包（RuntimeInformation 判定），
    包名后缀随之变成 win-x64 / linux-x64 / darwin-x64。产物是**本机**架构的
    （不传 cargo --target），故「在哪台机器上跑就出哪个平台的包」——
    这正是 CI 在 ubuntu runner 上出 linux-x64 包的方式。

.PARAMETER Version
    包版本号。默认取 workspace 的 version。

.PARAMETER OutDir
    输出目录。默认 `dist/`。

.PARAMETER TargetDir
    cargo 产物目录。默认取 $env:CARGO_TARGET_DIR 或 `target`。

.PARAMETER SkipBuild
    不重新编译，直接用已有的 release 产物。

.EXAMPLE
    pwsh -File scripts/build-package.ps1
    pwsh -File scripts/build-package.ps1 -Version 0.1.0 -SkipBuild
#>
[CmdletBinding()]
param(
    [string] $Version = '',
    [string] $OutDir = 'dist',
    [string] $TargetDir = '',
    [switch] $SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch { }

# ============================================================================
# 平台映射 —— 纯函数, 不读任何全局状态
# ============================================================================
#
# 为什么单独抽出来: 「包名后缀 + 二进制扩展名」是打包里唯一一处**平台相关**的
# 决策, 此前它是两行写死的 `-win-x64` 与 "$b.exe"。写死的后果不是「不优雅」,
# 是 **CI 上根本产不出包**: 集成方在 Linux/macOS 上拿不到任何二进制分发件。
#
# 纯函数 = 参数进、值出, 不碰 $IsWindows / $env:X / 当前目录。理由是可测:
# 只有纯函数才能在**任意**机器上被喂任意 (OSPlatform, Architecture) 组合 ——
# 而 CI runner 恰恰就是要在一台不是自己的机器上问「如果我是 Linux 我会得到什么」。
# 直接读全局状态的话, 这台 runner 永远只能验出它自己那一种组合。
#
# 契约:
#   Suffix     包名后缀, 沿用 Rust target triple 的约定 (win-x64/linux-x64/darwin-arm64)
#   Extension  可执行文件扩展名: Windows '.exe', 其余 '' (Unix 可执行文件无后缀)
#   RustTarget  cargo 完整 target triple, 供 BUILD-INFO 记录 (人可读的溯源信息)
#   IsWindows  决定文档/清理/示例该怎么写
#
# fail-closed: 不认识的 OS / 架构**抛异常**, 绝不编一个看起来合理的包名 ——
# 一个错后缀的包会让人以为「这个平台没做」, 而实际是「平台识别错了」。
function Get-PackageTarget {
    [CmdletBinding()]
    [OutputType([pscustomobject])]
    param(
        [Parameter(Mandatory = $true)] [string] $OSPlatform,
        [Parameter(Mandatory = $true)] [string] $Architecture
    )

    # switch 默认大小写不敏感, 故 'MacOS' / 'macos' 也认。
    $os = switch ($OSPlatform) {
        'Windows' { [pscustomobject]@{ Slug = 'win';    Ext = '.exe'; Triple = '{0}-pc-windows-msvc' } }
        'Linux'   { [pscustomobject]@{ Slug = 'linux';  Ext = '';     Triple = '{0}-unknown-linux-gnu' } }
        'macOS'   { [pscustomobject]@{ Slug = 'darwin'; Ext = '';     Triple = '{0}-apple-darwin' } }
        default {
            throw "不支持的操作系统: '$OSPlatform'。已实现 Windows / Linux / macOS; 其它平台没有经过验证的包名约定, 拒绝编造。"
        }
    }

    $arch = switch ($Architecture) {
        'X64'   { 'x64' }
        'Arm64' { 'arm64' }
        default {
            throw "不支持的架构: '$Architecture'。已实现 X64 / Arm64; 其它架构没有经过验证的包名约定, 拒绝编造。"
        }
    }

    $rustArch = if ($arch -eq 'x64') { 'x86_64' } else { 'aarch64' }

    [pscustomobject]@{
        OSPlatform  = $OSPlatform
        Architecture = $Architecture
        Suffix      = "$($os.Slug)-$arch"
        Extension   = $os.Ext
        RustTarget  = $os.Triple -f $rustArch
        IsWindows   = ($OSPlatform -eq 'Windows')
    }
}

# ---- 被 dot-source 时只取函数, 不执行下面的打包流程 ----
# 单测要 import 上面的函数, 而本脚本余下部分是**有副作用的**(Set-Location /
# cargo build / 删目录 / 写 zip)。不设这道闸, 测一个纯函数就会顺手把
# `dist/` 删一遍重建 —— 测试不该动生产路径。
# $MyInvocation.InvocationName 在 `pwsh -File x.ps1` / `& x.ps1` / `. x.ps1`
# 三种调用下分别是 <路径> / '&' / '.'(已实测)。
if ($MyInvocation.InvocationName -eq '.') { return }

$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

# ---- 当前平台 ----
# 用 RuntimeInformation 而不是 $IsWindows: 后者是 PowerShell 7.0+ 才有的
# 自动变量, 在 5.1 下是 $null, 而「在 5.1 下静默当成非 Windows」正是本脚本
# 此前最坏的形态。RuntimeInformation 来自 .NET, 与宿主 shell 版本无关。
$target = Get-PackageTarget `
    -OSPlatform  ([System.Runtime.InteropServices.RuntimeInformation]::OSPlatform) `
    -Architecture ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)
$exeExt = $target.Extension

# ---- 版本号 ----
if (-not $Version) {
    $cargoToml = [System.IO.File]::ReadAllText((Join-Path $repo 'Cargo.toml'), [System.Text.UTF8Encoding]::new($false))
    if ($cargoToml -notmatch '(?m)^\s*version\s*=\s*"([^"]+)"') { throw '无法从 Cargo.toml 解析 version' }
    $Version = $Matches[1]
}
$sha = (git rev-parse --short HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'git rev-parse 失败' }
$dirty = (git status --porcelain | Measure-Object).Count
$stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')

# ---- 目标目录 ----
if (-not $TargetDir) {
    $TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
}
$relBin = Join-Path $TargetDir 'release'

$pkgName = "im1.0-$Version-$($target.Suffix)"
$pkgRoot = Join-Path $repo (Join-Path $OutDir $pkgName)

Write-Host ''
Write-Host "=== 组装 $pkgName ===" -ForegroundColor Cyan
Write-Host "  版本   : $Version"
Write-Host "  commit : $sha$(if ($dirty -gt 0) { "  (工作树有 $dirty 处未提交改动 —— 这不该发生, 请先提交或 stash)" })"
Write-Host "  平台   : $($target.OSPlatform) / $($target.Architecture)  (rust target $($target.RustTarget))"
Write-Host "  产物   : $relBin"

# ---- 构建 ----
if (-not $SkipBuild) {
    Write-Host ''
    Write-Host '--- cargo build --release ---' -ForegroundColor Cyan
    # **不传 --target**: 刻意构建**本机**产物。
    # ① 传了 --target, 产物会落到 target/<triple>/release/, 于是 -SkipBuild
    #    与「复用别处已构建产物」全部失效(而这两种用法是本脚本的既有契约)。
    # ② 交叉编译需要目标平台的链接器; 本仓依赖 sqlx/postgres, 交叉链接不是
    #    配个 --target 就能保证的事。CI 的用法是「在哪台 runner 上跑就出哪个
    #    平台的包」, 天然就是本机构建, 不需要交叉。
    & cargo build --release --locked -p im-gateway -p im-migrate -p jobctl
    if ($LASTEXITCODE -ne 0) { throw "cargo build 失败 (exit=$LASTEXITCODE)" }
}

# ---- 清空并建目录树 ----
if (Test-Path -LiteralPath $pkgRoot) {
    # 同 test-preflight: `dist/` 是 .gitignore 覆盖的构建产物目录, 整个重建,
    # 走回收站没有意义。清理**不依赖**本机的可恢复删除工具 —— 那个工具只存在
    # 于某一台 Windows 机器上, 依赖它会让脚本在 Linux CI runner 上跑不了
    # (run 37348672697 就是这么红的)。
    Write-Host ''
    Write-Host "  已存在 $pkgName，正在删除并重建" -ForegroundColor Yellow
    Remove-Item -LiteralPath $pkgRoot -Recurse -Force -ErrorAction Stop
}
# 非 Windows 包不装 .ps1 助手(见下方「平台说明」一节的取舍), 但目录树保持
# 四个子目录, 免得两套布局让下游脚本/文档各写一份。
foreach ($d in @('bin', 'config', 'scripts', 'docs')) {
    New-Item -ItemType Directory -Path (Join-Path $pkgRoot $d) -Force | Out-Null
}

# ---- 拷二进制 ----
$bins = @{}
foreach ($b in @('im-gateway', 'im-migrate', 'jobctl')) {
    $file = "$b$exeExt"
    $src = Join-Path $relBin $file
    if (-not (Test-Path -LiteralPath $src)) {
        throw "缺少产物 $src —— 先构建再打包(本包目标平台 $($target.OSPlatform)/$($target.Architecture), 可执行文件后缀 '$exeExt')"
    }
    Copy-Item -LiteralPath $src -Destination (Join-Path (Join-Path $pkgRoot 'bin') $file)
    $bins[$b] = (Get-FileHash -LiteralPath $src -Algorithm SHA256).Hash
    Write-Host "  bin/$file"
}

# ---- 拷模板 / 文档 ----
# 一律 Join-Path: 字符串里的字面量反斜杠在 Linux/macOS 上**不是**路径分隔符,
# 'config\env.example' 会是一个名为 `config\env.example` 的单层文件名 ——
# 目录看起来装好了, 里面其实是空的。
Copy-Item -LiteralPath (Join-Path $repo 'packaging/template/env.example') -Destination (Join-Path (Join-Path $pkgRoot 'config') 'env.example')
Copy-Item -LiteralPath (Join-Path $repo 'packaging/template/INSTALL.md')   -Destination (Join-Path $pkgRoot 'INSTALL.md')
$psHelpers = @('install.ps1', 'preflight.ps1', 'start-gateway.ps1')
if ($target.IsWindows) {
    foreach ($s in $psHelpers) {
        Copy-Item -LiteralPath (Join-Path $repo "packaging/template/scripts/$s") -Destination (Join-Path (Join-Path $pkgRoot 'scripts') $s)
    }
} else {
    # 非 Windows 包**不带**这三个 .ps1: 它们内部写死了 `bin\im-gateway.exe`
    # 与 `bin\jobctl.exe`(install.ps1:39 / preflight.ps1:279 / start-gateway.ps1:38),
    # 在只有 `bin/im-gateway` 的包里必然找不到目标, 而这三个文件不是本 lane 的
    # 独占文件、不能改。发一份跑不起来的安装脚本比不发更糟 —— operator 会以为
    # 是自己环境的问题。替代用法写在包内 PLATFORM.md 里。
    Write-Host "  (非 Windows 目标: 不打包 .ps1 助手, 改由 PLATFORM.md 给出等价命令)"
}
Copy-Item -LiteralPath (Join-Path $repo 'docs/api/QUICKSTART.md') -Destination (Join-Path (Join-Path $pkgRoot 'docs') 'QUICKSTART.md')
Copy-Item -LiteralPath (Join-Path $repo 'docs/api/openapi.json')  -Destination (Join-Path (Join-Path $pkgRoot 'docs') 'openapi.json')
Copy-Item -LiteralPath (Join-Path $repo 'docs/api/asyncapi.json') -Destination (Join-Path (Join-Path $pkgRoot 'docs') 'asyncapi.json')
$license = Get-ChildItem -Path $repo -Filter 'LICENSE*' -File | Select-Object -First 1
if ($license) { Copy-Item -LiteralPath $license.FullName -Destination (Join-Path $pkgRoot 'LICENSE') }
Write-Host '  config/env.example / docs/* / INSTALL.md'

# ---- PLATFORM.md（包内平台说明）----
# 为什么非 Windows 包**必须**有它: INSTALL.md 是仓内模板, 里面写死了
# 「Windows x86_64」「bin\im-gateway.exe」「pwsh -File scripts\install.ps1」。
# 发给 Linux 接入方时, 那份文档会教他跑一个不存在的文件 —— 拿不到包只是「不方便」,
# 拿到一份教他跑错文件的文档是「他以为自己配错了」。
#
# 只在非 Windows 时生成: Windows 包里 INSTALL.md 的每一句都成立, 再塞一份
# PLATFORM.md 是噪音。
if (-not $target.IsWindows) {
    $pf = @()
    $pf += "# im1.0 安装说明（$($target.OSPlatform) $($target.Architecture)）"
    $pf += ''
    $pf += '本包是**预编译分发包**: 解压即用, 不需要 Rust 工具链、不需要 Docker。'
    $pf += "包名后缀 -$($target.Suffix) 就是目标平台, 装错平台会直接 exec format error。"
    $pf += ''
    $pf += '**本文件在冲突处优先于同目录的 INSTALL.md。** INSTALL.md 的第 6 节'
    $pf += '(配置速查)与第 8 节(常见问题)与平台无关, 仍然有效; 它的第 1/3/4/5/7 节'
    $pf += '按 Windows 写, 请以下面这份为准。'
    $pf += ''
    $pf += '## 1. 校验完整性'
    $pf += ''
    $pf += '```sh'
    $pf += 'sha256sum -c SHA256SUMS.txt   # 在本包根目录下执行'
    $pf += '```'
    $pf += ''
    $pf += '## 2. 准备配置'
    $pf += ''
    $pf += '```sh'
    $pf += 'cp config/env.example .env && 编辑 .env'
    $pf += '```'
    $pf += ''
    $pf += 'env.example 本身与平台无关(全是环境变量), 逐项说明见 INSTALL.md 第 6 节。'
    $pf += '唯一规则: 变量名去掉 `IM_` 前缀后小写, 必须等于 `AppConfig` 的字段名。'
    $pf += ''
    $pf += '## 3. 数据库迁移'
    $pf += ''
    $pf += '```sh'
    $pf += './bin/im-migrate --database-url "$IM_POSTGRES_URL"'
    $pf += '```'
    $pf += ''
    $pf += '## 4. 启动网关'
    $pf += ''
    $pf += '```sh'
    $pf += 'export IM_POSTGRES_URL=postgres://user:password@127.0.0.1:5432/im1'
    $pf += '# ...其余变量照 config/env.example 填好并 export...'
    $pf += './bin/im-gateway'
    $pf += '```'
    $pf += ''
    $pf += '**.env 读失败是静默的**(dotenvy 的返回值被丢弃), 变量不全时你只会看到'
    $pf += '`config load failed: internal error` —— 这句话不包含任何真实原因。'
    $pf += '根因与取舍见 INSTALL.md 第 5 节: 修复它会把密钥打到 stderr, 故本仓选择'
    $pf += '把问题拦在进进程之前。包内的 Windows 版 preflight.ps1 做了这件事,'
    $pf += '但它读的是 bin\im-gateway.exe, 在本包不存在 —— 所以**配置必须手工核对**。'
    $pf += ''
    $pf += '## 5. 运维 CLI'
    $pf += ''
    $pf += '```sh'
    $pf += './bin/jobctl dlq list'
    $pf += './bin/jobctl dlq replay --id <dlq_id>'
    $pf += '```'
    $pf += ''
    $pf += '## 6. 如果提示 Permission denied'
    $pf += ''
    $pf += '```sh'
    $pf += 'chmod +x bin/*'
    $pf += '```'
    $pf += ''
    $pf += 'zip 格式在 Unix 上不带可执行位, 解压出来的二进制默认可能是 0644。'
    $pf += '打包脚本会尝试把 0755 写进 zip 的 external attributes, 但**不是所有'
    $pf += '解压工具都会照做** —— 这一条命令永远有效。'
    [System.IO.File]::WriteAllLines((Join-Path $pkgRoot 'PLATFORM.md'), $pf, [System.Text.UTF8Encoding]::new($false))
    Write-Host '  PLATFORM.md (非 Windows 平台说明)'
}

# ---- BUILD-INFO ----
# rustc 是**可选**的: 本仓没有 rust-toolchain.toml, 而打包常常发生在
# 「只装了 cargo 运行时 / 复用别处产物 / CI 缓存」的环境里。拿不到就写
# 未知, 不能因为一条元信息让整个打包失败 —— 那会让「打包」在某些环境下
# 永远做不成, 而它本来是可以做的。
$rustcVer = '未知（构建环境未提供 rustc）'
if (Get-Command rustc -ErrorAction SilentlyContinue) {
    $rustcVer = (& rustc --version)
    if ($LASTEXITCODE -ne 0 -or -not $rustcVer) { $rustcVer = '未知（rustc 调用失败）' }
}

$bi = @()
$bi += "im1.0 分发包构建信息"
$bi += ''
$bi += "版本      : $Version"
$bi += "commit     : $sha"
$bi += "构建时间(UTC): $stamp"
# 「目标平台」是本节最关键的一行: operator 拿到的第一个问题永远是
# 「这个包能不能在我的机器上跑」。此前这里写死 windows x86_64, 于是 Linux 包
# (以及将来任何包) 的这一行都在说谎。
$bi += "目标平台  : $($target.OSPlatform) $($target.Architecture)"
$bi += "包名后缀  : $($target.Suffix)"
$bi += "Rust target: $($target.RustTarget)"
$bi += "构建主机  : $([System.Runtime.InteropServices.RuntimeInformation]::OSDescription)"
$bi += "Rust 工具链: $rustcVer"
$bi += ''
$bi += '二进制 SHA256:'
foreach ($k in $bins.Keys) { $bi += ("  {0,-12} {1}" -f $k, $bins[$k]) }
$bi += ''
$bi += '本包不含 migrations/ 目录: 迁移 SQL 由 sqlx::migrate! 编译期内嵌进'
$bi += 'im-migrate 二进制，二进制与其携带的 schema 必然同版本。'
$bi += ''
$bi += '校验整个包:'
if ($target.IsWindows) {
    $bi += '  pwsh -c "Get-Content SHA256SUMS.txt | ForEach-Object { $h,$p = $_ -split ''  '',2; if ((Get-FileHash $p -Algorithm SHA256).Hash.ToLower() -ne $h) { Write-Host \"BAD $p\" } }"'
} else {
    $bi += '  sha256sum -c SHA256SUMS.txt'
}
$bi += '  与本文件及 SHA256SUMS.txt 比对'
[System.IO.File]::WriteAllLines((Join-Path $pkgRoot 'BUILD-INFO.txt'), $bi, [System.Text.UTF8Encoding]::new($false))

# ---- SHA256SUMS（覆盖包内每个文件）----
$sumLines = Get-ChildItem -LiteralPath $pkgRoot -Recurse -File |
    Sort-Object FullName |
    ForEach-Object {
        # GetRelativePath 而不是 Substring($pkgRoot.Length + 1): 后者切的是
        # Windows 上 FullName 的形状(假定分隔符是 '\'), 在 Linux 上 FullName 用
        # '/', 切出来的相对路径混着两种分隔符, SHA256SUMS.txt 里的路径就没法用。
        $rel = [System.IO.Path]::GetRelativePath($pkgRoot, $_.FullName) -replace '\\', '/'
        '{0}  {1}' -f (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLower(), $rel
    }
[System.IO.File]::WriteAllLines((Join-Path $pkgRoot 'SHA256SUMS.txt'), $sumLines, [System.Text.UTF8Encoding]::new($false))

# ---- 压成 zip ----
# Compress-Archive 保留: 它随 pwsh 分发, 在 Linux/macOS 上同样可用, 且实测
# (pwsh 7.6.6) 写出的 entry 名已是 '/' 分隔, 与 ZipFile::CreateFromDirectory
# 逐条相同 —— 换 API 只会是无收益的 churn。
$zip = Join-Path $repo (Join-Path $OutDir "$pkgName.zip")
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force -ErrorAction Stop }
Compress-Archive -Path $pkgRoot -DestinationPath $zip -CompressionLevel Optimal

# zip 不带 Unix 可执行位: 解压出来的 bin/* 是 0644, `./bin/im-gateway` 直接
# Permission denied。把 0755 写进 external attributes 的高 16 位 —— 那是 zip
# 存 Unix mode 的约定位置。刻意**不因此让打包失败**: 解压工具各有各的脾气,
# PLATFORM.md 里已经给了 chmod +x 这条永远有效的兜底。
if (-not $target.IsWindows) {
    try {
        Add-Type -AssemblyName System.IO.Compression.FileSystem -ErrorAction Stop
        $za = [System.IO.Compression.ZipFile]::Open($zip, [System.IO.Compression.ZipArchiveMode]::Update)
        try {
            foreach ($e in $za.Entries) {
                if ($e.Name -in @('im-gateway', 'im-migrate', 'jobctl')) {
                    $e.ExternalAttributes = ($e.ExternalAttributes -bor 0x1ED) -shl 16
                }
            }
        } finally { $za.Dispose() }
    } catch {
        Write-Warning "无法写入 zip 可执行位(不影响打包成功, 解压后 chmod +x bin/* 即可): $_"
    }
}

$zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash
$zipInfo = Get-Item -LiteralPath $zip
Write-Host ''
Write-Host '=== 完成 ===' -ForegroundColor Green
Write-Host "  目录 : $pkgRoot"
Write-Host "  压缩 : $zip  ($([math]::Round($zipInfo.Length / 1MB, 2)) MB)"
Write-Host "  SHA256: $zipHash"
Write-Host ''
Write-Host '  发布前请人工确认:'
Write-Host '   1. 包里**没有** .env 或任何密钥（SHA256SUMS.txt 里应只有 env.example）'
Write-Host '   2. BUILD-INFO.txt 里的 commit 与你要发布的 commit 一致'
Write-Host "   3. BUILD-INFO.txt 的「目标平台」与包名后缀 -$($target.Suffix) 一致"
if ($target.IsWindows) {
    Write-Host '   4. 在一台**没有** Rust 工具链的机器上跑 scripts/install.ps1 -SkipMigrate 能过自检'
} else {
    Write-Host '   4. 在一台**没有** Rust 工具链的机器上, chmod +x bin/* 后 ./bin/im-gateway 能起来'
}
exit 0
