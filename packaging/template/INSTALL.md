# im1.0 安装说明（Windows x86_64）

本包是**预编译分发包**：解压即用，不需要 Rust 工具链、不需要 Docker。

---

## 1. 系统要求

| 项 | 要求 |
|---|---|
| 操作系统 | Windows x86_64（Windows 10 / 11 / Server 2019+） |
| PowerShell | **7.0 或更高**（`pwsh -v` 查看）。Windows 自带的 `powershell.exe` 5.1 **不支持**，本包脚本会直接拒绝。 |
| PostgreSQL | 15 或更高 |
| 内存 | 建议 ≥ 512 MB 可用 |
| 端口 | 8080（或用 `IM_HTTP_PORT` 改） |

> 事件总线：MVP 默认 `kind=stub`，**不需要**装 NATS。事件会被有意丢弃，这是配置决定不是故障。

---

## 2. 校验包完整性（可选但建议）

```powershell
Get-FileHash .\im1.0-<版本>-win-x64.zip -Algorithm SHA256
# 与发布说明里的值比对
```

解压后可逐文件校验：

```powershell
cd im1.0-<版本>-win-x64
Get-Content SHA256SUMS.txt | ForEach-Object {
  $h, $p = $_ -split '  ', 2
  $a = (Get-FileHash $p -Algorithm SHA256).Hash.ToLower()
  "{0}  {1}" -f $(if ($a -eq $h) { 'OK  ' } else { 'BAD ' }), $p
}
```

`BUILD-INFO.txt` 记录了这个包是从哪个 commit、哪个 Rust 版本、什么时间构建的。报障时请附上它。

---

## 3. 安装

```powershell
pwsh -File scripts\install.ps1
```

它做三件事，顺序不能换：

1. 从 `config\env.example` 生成 `.env`（已存在则不覆盖，除非加 `-Force`）
2. **先跑配置自检**，列出 `.env` 里还剩哪些占位符没换
3. 配置通过后跑数据库迁移

自检必须在迁移之前 —— 用一个错的连接串去建表，失败信息会指向 schema 而不是配置，排查方向会跑偏。

只做配置自检、不碰数据库：

```powershell
pwsh -File scripts\install.ps1 -SkipMigrate
```

生成随机密钥（填进 `.env`）：

```powershell
-join ((1..48) | ForEach-Object { '0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ'[(Get-Random -Maximum 62)] })
```

---

## 4. 启动

```powershell
pwsh -File scripts\start-gateway.ps1
```

它会先自检再启动。**不要**直接跑 `bin\im-gateway.exe` —— 见下面「为什么不能直接跑」。

换端口：

```powershell
pwsh -File scripts\start-gateway.ps1 -Port 9090
```

---

## 5. 为什么不能直接跑 `bin\im-gateway.exe`

两个原因，都是实测出来的：

**① `.env` 读失败是静默的。**
`crates/im-common/src/config.rs:289` 写的是 `let _ = dotenvy::dotenv();` —— 返回值被丢弃。
`.env` 路径不对、编码不对、语法坏了，dotenvy 都不报错，服务照常启动然后失败，
而你看到的只有一句 `config load failed: internal error`。

**② 那句 `internal error` 什么信息都没有。**
根因是 `crates/im-common/src/error.rs:232` 的 `#[error("internal error")]` 漏了 `{0}` 占位符，
figment 的完整诊断（哪个字段缺失、值哪里不对）被 `Display` 整个丢掉了。

**为什么不直接修那一行**：figment 解析 `IM_JWT_SIGNING_KEYS` 失败时会在诊断里**回显输入值**，
而输入值就是 JWT 签名密钥原文。把 `{0}` 加回去等于把密钥打到 stderr —— 踩「凭据永不打印」这条红线。

所以修法是**在进进程之前把问题拦下来**：`scripts\preflight.ps1` 用 PowerShell 自己做一遍校验，
逐项报出可操作的结论，**任何情况下都不打印变量的值**（只报长度）。

它还会真跑一次二进制，确认配置确实能加载过 —— 静态校验和真实行为两边对上才算数。

---

## 6. 配置速查

只有一条规则：**变量名去掉 `IM_` 前缀后小写，必须等于 `AppConfig` 的字段名。**

| 环境变量 | 必填 | 说明 |
|---|---|---|
| `IM_POSTGRES_URL` | ✅ | `postgres://user:password@host:port/dbname` |
| `IM_JWT_SIGNING_KEYS` | ✅ | JSON 数组，至少 1 项，每项含 `kid` + `key`，至少一项 `active:true` |
| `IM_REFRESH_PEPPER` | ✅ | 与 sha256 一起派生 refresh token 哈希。**换值会让所有已签发的 refresh token 失效** |
| `IM_EVENT_PUBLISHER` | ✅ | JSON 对象：`{"kind":"stub","nats_url":""}` 或 `{"kind":"nats","nats_url":"nats://..."}` |
| `IM_HTTP_PORT` | | 默认 `8080` |
| `IM_SERVER_SECRETS` | | JSON 对象 `{"<env-uuid>":"secret"}`，不配则 S2S 接口不可用 |
| `IM_ACCESS_TOKEN_TTL_SECONDS` | | 默认 `900` |
| `IM_REFRESH_TOKEN_TTL_SECONDS` | | 默认 `2592000` |
| `IM_MAX_MESSAGE_SIZE_BYTES` | | 默认 `65536` |

### 几个**错的**名字（本仓历史文档里出现过，别用）

| 错误 | 正确 |
|---|---|
| `IM_EVENT_PUBLISHER_KIND` | `IM_EVENT_PUBLISHER`（整块 JSON） |
| `IM_EVENT_PUBLISHER_NATS_URL` | 同上 |
| `IM__JWT__SIGNING__KEYS` | `IM_JWT_SIGNING_KEYS`（双下划线是嵌套分隔符，本仓字段是单层） |
| `IM__REFRESH__PEPPER` | `IM_REFRESH_PEPPER` |
| `IM_DATABASE_URL` | `IM_POSTGRES_URL` |
| `DATABASE_URL` | `IM_POSTGRES_URL`（前者只有测试代码读） |

**不认识的 `IM_*` 变量会被静默忽略**（实测：设一个完全虚构的 `IM_TOTALLY_BOGUS_KEY`，服务照常启动）。
所以「我配了」不等于「它生效了」，改完配置请跑一次 preflight。

---

## 7. 运维 CLI

```powershell
# 列死信
bin\jobctl.exe dlq list
bin\jobctl.exe dlq list --task im.message.created --limit 20

# 重放
bin\jobctl.exe dlq replay --id <dlq_id>

# 丢弃（不可逆）
bin\jobctl.exe dlq discard --id <dlq_id> --reason "..."
```

`jobctl` 与 `im-gateway` 读**同一份** `IM_POSTGRES_URL`，不另设变量。
`replay` 需要 `IM_EVENT_PUBLISHER` 里 `kind=nats` 且有 `nats_url`，否则会拒绝并说明原因。

---

## 8. 常见问题

**启动报 `config load failed: internal error`**
先跑 `pwsh -File scripts\preflight.ps1`，它会指出具体是哪个变量。

**`/readyz` 返回 503**
正常反映数据库或事件总线不可用。`/healthz` 是进程存活探针，不查依赖。

**guest 注册后 WebSocket 连上就断**
首帧必须是 `{"type":"auth","access_token":"...","req_id":"..."}`。`req_id` 可省略。
认证成功回的是 `auth_ok` 帧 —— 它**不在**已定义的 `ServerFrame` 枚举里，
是网关手工构造的，规范尚未定义，见 `docs\QUICKSTART.md` 的「还没实现的」一节。

**改了 `IM_EVENT_PUBLISHER` 但事件仍不投递**
确认 `kind` 是 `nats` 而不是 `stub`。`stub` 是**配置决定**丢弃事件，不是故障，
此时 `/readyz` 不会因此变红。
