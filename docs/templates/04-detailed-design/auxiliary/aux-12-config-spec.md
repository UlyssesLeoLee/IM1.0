---
doc_id: aux-12
title_ja: 構成仕様書 (IM1.0)
title_zh: 配置项规格 (IM1.0) - IM1.0 27 项环境变量 + 10 K3s Secret Key 完整清单
phase: 04-detailed-design-aux
owners: 架构师 (Mavis 接手 agent per DEC-008) + SRE + Tech Lead
status: Filled (v1.0.0)
version: 1.1.0
related_activities: 35 Infra 基本, 53 开发环境, 104 本番环境
---

# aux-12. 構成仕様書 (IM1.0) / 配置项规格 (IM1.0)

> 项目: **IM1.0**
> 阶段: 詳細設計 / Detailed Design(辅助) — 已填实版本
> 责任方: 架构师 + SRE + Tech Lead
> 配置源: `docs/ImplementationSpec.md` §6(27 个环境变量 + 10 个 K3s Secret Key + 加载顺序 + 校验规则)+ `docs/BasicDesign.md` §14.3(9 项 environment.settings)
> 命名规范: `aux-01-naming-convention.md` §A(业务术语)
> 错误码: `aux-03 §B`

## 1. 目的 (Purpose)

为 IM1.0 全部运行期配置(env vars + K3s Secret Key + `environments.settings`)建立统一规格,涵盖分类 / 加载 / 校验 / 热更新 / 密钥 / 环境差异。本表是 `AppConfig::load()` 实装的唯一真源(`WBS D-1` 任务)。

## 2. 适用范围 (Scope)

| 范围 | 数量 | 章节 |
|---|---|---|
| 必填环境变量 | 9 项 | §A.1 |
| 可调环境变量 | 15 项 | §A.2 |
| 可观测环境变量 | 3 项 | §A.3 |
| **小计 (env vars)** | **27 项** | (ImplementationSpec §1.1 第 38 行明确"27 个") |
| K3s Secret Key(per-env × 4) | 4 项 | §B.1 |
| K3s Secret Key(shared × 6) | 6 项 | §B.2 |
| **小计 (K3s Secret)** | **10 项** | (ImplementationSpec §1.1 第 39 行明确"10 个") |
| `environments.settings` 字段 | 9 项 | §C(BasicDesign §14.3) |
| 合计 | **46 项** | |

## 3. 责任方 (Owners)

架构师(定义 + 维护)+ SRE(部署 + 校验 + 告警)+ Tech Lead(实现 `AppConfig::load()`)。新增配置项需 3 人之一 + PM 同意。

## 4. 前置依赖 (Prerequisites / Inputs)

- 仓库结构: `ImplementationSpec §2.1`
- Cargo workspace 依赖: `ImplementationSpec §2.2`(`figment = 0.10` + `dotenvy = 0.15` + `secrecy = 0.8`)
- DB schema: `migrations/0001-0006` + `aux-02 §F`
- 错误码: `aux-03 §B`
- 部署: `ImplementationSpec §8.4` K3s + GitHub Actions
- 命名规范: `aux-01 §D`

## 5. 输出 / 模板正文 (Body)

## A. IM1.0 环境变量清单(27 项)

> 命名约定: `IM_<CATEGORY>_<NAME>`(snake_case),`IM_` 前缀避免与系统 env 冲突(per `aux-01 §D`)

### A.1 必填环境变量(9 项,缺失 → 进程 exit 78)

| 编号 | 名称 | 类型 | 默认 | 必填 | 校验 | 类别 | 用途 / 关联实现 |
|---|---|---|---|---|---|---|---|
| 1 | `IM_ENV` | string | — | Y | ∈ {`dev`, `staging`, `prod`} | env | 决定加载 `config/{IM_ENV}.toml` + 日志格式 + 限流严格度 |
| 2 | `IM_DATABASE_URL` | string | — | Y | 可 psql 连接(启动 ping 一次) | env | PG 18.6 连接 URL,共享 |
| 3 | `IM_VALKEY_URL` | string | — | Y | `redis://` 协议 | env | 限流 + `environments.settings` 缓存,共享 |
| 4 | `IM_NATS_URL` | string | — | Y | `nats://` 协议 | env | 事件总线(MVP 占位,V1+ 真实) |
| 5 | `IM_HTTP_PORT` | int | 8080 | Y | 1-65535 | env | im-gateway HTTP 端口 |
| 6 | `IM_GRPC_PORT` | int | 9000 | Y | 1-65535 | env | im-core gRPC Server 端口(im-gateway 调用) |
| 7 | `IM_JWT_SIGNING_KEYS` | JSON | — | Y | 合法 JSON 数组,每项 `{kid: string, key: hex(64)}`,至少 1 项 | env(vault) | JWT 签名密钥,轮换期 2 项 |
| 8 | `IM_SERVER_SECRETS` | JSON map | — | Y | 合法 JSON map,key 为 env UUID,value 为 hex(64) | env(vault) | Server-to-Server HMAC 密钥,per-env |
| 9 | `IM_REFRESH_TOKEN_PEPPER` | string | — | Y | hex(64) | env(vault) | Refresh Token 哈希 pepper |

> **来源**: `ImplementationSpec §1.1` 第 38 行 "27 个环境变量(必填 9 + 可调 15 + 可观测 3)" + `§6.2` 校验表

### A.2 可调环境变量(15 项,有合理默认值)

| 编号 | 名称 | 类型 | 默认 | 范围 | 必填 | 类别 | 关联实现 / 算法 |
|---|---|---|---|---|---|---|---|
| 10 | `IM_LOG_LEVEL` | string | `info,sqlx=warn` | ∈ {`trace`,`debug`,`info`,`warn`,`error`} + `EnvFilter` 语法 | N | center | 日志等级 + 模块过滤 |
| 11 | `IM_WS_PING_INTERVAL_SECONDS` | int | 30 | ≥ 5 | N | center | WS 客户端 ping 周期 |
| 12 | `IM_WS_HEARTBEAT_TIMEOUT_SECONDS` | int | 60 | > `IM_WS_PING_INTERVAL_SECONDS` | N | center | WS 服务端超时断开 |
| 13 | `IM_WS_AUTH_TIMEOUT_MS` | int | 100 | 50-5000 | N | center | WS 鉴权超时(防滥用) |
| 14 | `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` | int | 60 | ≥ 1 | N | center | 单用户发消息限流 |
| 15 | `IM_RATE_LIMIT_GUEST_REGISTER_PER_HOUR` | int | 10 | ≥ 1 | N | center | Guest 注册限流 |
| 16 | `IM_RATE_LIMIT_TOKEN_EXCHANGE_PER_MIN` | int | 1000 | ≥ 1 | N | center | Token Exchange 限流 |
| 17 | `IM_RATE_LIMIT_BUCKET_SIZE` | int | 100 | ≥ 1 | N | center | 全局 req/s/user |
| 18 | `IM_MESSAGE_MAX_SIZE_BYTES` | int | 65536 (64KB) | 1024 - 1048576 (1KB - 1MB) | N | center | 消息 content 大小上限 |
| 19 | `IM_MESSAGE_RECALL_WINDOW_SECONDS` | int | 120 | ≥ 1 | N | center | 撤回时间窗(可被 env.settings 覆盖) |
| 20 | `IM_ACCESS_TOKEN_TTL_SECONDS` | int | 900 (15min) | ≥ 60 | N | center | Access Token TTL |
| 21 | `IM_REFRESH_TOKEN_TTL_SECONDS` | int | 2592000 (30d) | ≥ 86400 | N | center | Refresh Token TTL |
| 22 | `IM_DB_POOL_MAX_CONNECTIONS` | int | 20 | 1-200 | N | center | sqlx 连接池上限 |
| 23 | `IM_AUTH_MIDDLEWARE_CACHE_SECONDS` | int | 5 | 0-60 | N | center | Token 校验缓存(0 = 关闭) |
| 24 | `IM_GUEST_USER_DEFAULT_NAME` | string | `Guest` | max 64 | N | env | Guest 默认显示名 |

> **类别说明**:
> - `env`:仅环境变量注入,不可热更
> - `center`:可走配置中心(NATS pub/sub)或 env 注入,**可热更** 业务参数
> - `vault`:K3s Secret / Vault,需重连才能更新

### A.3 可观测环境变量(3 项)

| 编号 | 名称 | 类型 | 默认 | 必填 | 类别 | 用途 |
|---|---|---|---|---|---|---|
| 25 | `IM_PROMETHEUS_BIND` | string | (空 = 关闭) | N | env | Prometheus 端点 bind,例 `0.0.0.0:9100` |
| 26 | `IM_OTEL_EXPORTER_OTLP_ENDPOINT` | string | (空 = 关闭) | N | env | OpenTelemetry OTLP endpoint,默认关闭(V1+ 启用) |
| 27 | `IM_SERVICE_NAME` | string | (auto,`CARGO_PKG_NAME`) | N | env | 服务名,用于 `service.name` 资源属性 |

> **来源**: `ImplementationSpec §1.1` 第 38 行 + `§6.2` 校验表

## B. K3s Secret Key 清单(10 项)

> 命名约定: K8s Secret 名 `im-env-{environment_id}`(per-env)+ `im-shared-*`(shared)
> 引用: `ImplementationSpec §6.4` + §1.1 第 39 行

### B.1 Per-Environment Secret Keys(4 项 × N env)

> 每个 env 一个 Secret,名为 `im-env-{environment_id}`

| 编号 | Secret Key | 注入到 env | 用途 | 轮换周期 | 关联 |
|---|---|---|---|---|---|
| 28 | `server_secret` | (不直接注入,经 `IM_SERVER_SECRETS` map 加载) | Server-to-Server HMAC | 季度 | `ImplementationSpec §3.1.1` + `aux-11 §2` |
| 29 | `jwt_signing_key_v1` | (同上,`IM_JWT_SIGNING_KEYS` 数组第 1 项) | JWT 签名 v1 | 轮换期 7 天观察 | `aux-06 §B A-010` POC-03 |
| 30 | `jwt_signing_key_v2` | (同上,轮换期第 2 项) | JWT 签名 v2(轮换用) | 轮换期 7 天观察 | 同上 |
| 31 | `refresh_token_pepper` | `IM_REFRESH_TOKEN_PEPPER` | Refresh Token 哈希 pepper | 季度 | `aux-06 §B A-002` |

### B.2 Shared Secret Keys(6 项)

> 共享 Secret,全 env 共用,名为 `im-shared-*`

| 编号 | Secret Key | 注入到 env | 用途 | 轮换周期 |
|---|---|---|---|---|
| 32 | `database_url` | `IM_DATABASE_URL` | PG 18.6 连接 | V1+ 评估(密码季度) |
| 33 | `valkey_url` | `IM_VALKEY_URL` | 限流 + 缓存 | V1+ 评估 |
| 34 | `nats_url` | `IM_NATS_URL` | 事件总线 | V1+ 评估 |
| 35 | `minio_endpoint` | `IM_MINIO_ENDPOINT` | 媒体 S3 兼容 endpoint | V1+ 评估 |
| 36 | `minio_access_key` | `IM_MINIO_ACCESS_KEY` | MinIO access | V1+ 评估 |
| 37 | `minio_secret_key` | `IM_MINIO_SECRET_KEY` | MinIO secret | V1+ 评估 |

> **来源**: `ImplementationSpec §1.1` 第 39 行 "10 个 K3s Secret Key(4 个 per-env:server_secret / jwt_v1 / jwt_v2 / refresh_pepper;6 个 shared:db / valkey / nats / minio_endpoint / minio_access / minio_secret)"

## C. `environments.settings` JSONB 字段(9 项)

> 存于 `environments` 表,运行时通过 `SettingsService` 加载到 Valkey 缓存(`ImplementationSpec §6.3` + `BasicDesign §14.3`)
> 引用: `aux-04 §B.1` + `migrations/0001` 第 35 行 + `aux-02 §F.3`

| 编号 | 字段路径 | 类型 | 默认 | 用途 | 关联 |
|---|---|---|---|---|---|
| 38 | `friend_system_enabled` | bool | `true` | 是否启用好友系统 | `aux-04 §B.2/B.3` |
| 39 | `rate_limit.send_message_per_min` | int | 60 | 单用户发消息限流(env 级覆盖) | `ImplementationSpec §11.3` |
| 40 | `rate_limit.guest_register_per_hour` | int | 10 | Guest 注册限流(env 级覆盖) | 同上 |
| 41 | `message.recall_window_seconds` | int | 120 | 撤回时间窗(env 级覆盖) | `aux-04 §B.4` + `aux-11 §6` |
| 42 | `message.retention_days.text` | int | 365 | 文本消息留存期(V1+) | 法规要求 |
| 43 | `message.retention_days.media` | int | 90 | 媒体消息留存期(V1+) | 同上 |
| 44 | `voice.enabled` | bool | `false` | 是否启用语音(V1+,MVP 关闭) | `LiveKit-Voice-Subsystem.md` |
| 45 | `audit.detailed` | bool | `false` | 详细审计(写入 detail JSONB) | `migrations/0006` |
| 46 | `feature.experimental_flags` | object | `{}` | 实验性特性开关(预留) | V1+ |

> **来源**: `BasicDesign §14.3` 9 项 settings + `migrations/0001` 第 35 行 `settings JSONB`

## D. 校验规则(完整 11 条)

> 全部 11 条 from `ImplementationSpec §6.2` 校验表

| 变量 | 校验 | 失败行为 |
|---|---|---|
| `IM_DATABASE_URL` | 必须能 psql 连接(启动 ping) | exit 78 |
| `IM_JWT_SIGNING_KEYS` | 合法 JSON 数组,每项 `{kid, key:hex(64)}`,≥ 1 项 | exit 78 |
| `IM_SERVER_SECRETS` | 合法 JSON map,key=env UUID,value=hex(64) | exit 78 |
| `IM_REFRESH_TOKEN_PEPPER` | hex(64) | exit 78 |
| `IM_HTTP_PORT` / `IM_GRPC_PORT` | 1-65535 | exit 78 |
| `IM_ENV` | ∈ {`dev`, `staging`, `prod`} | exit 78 |
| `IM_LOG_LEVEL` | ∈ {`trace`, `debug`, `info`, `warn`, `error`} + `EnvFilter` 语法 | exit 78 |
| `IM_WS_HEARTBEAT_TIMEOUT_SECONDS` | > `IM_WS_PING_INTERVAL_SECONDS`(默认 60 > 30) | exit 78 |
| `IM_RATE_LIMIT_*` | ≥ 1 | exit 78 |
| `IM_MESSAGE_MAX_SIZE_BYTES` | 1024 ≤ x ≤ 1048576 (1KB - 1MB) | exit 78 |
| `IM_DB_POOL_MAX_CONNECTIONS` | 1 ≤ x ≤ 200 | exit 78 |

> 启动失败统一走 `process::exit(78)`(sysexists.h `EX_CONFIG`)。CI 阻断 + 启动日志记录原因。

## E. 加载顺序(7 步,per `ImplementationSpec §6.1`)

```
1. 读取 IM_ENV 决定加载 config/{IM_ENV}.toml(可选)
2. 读取 .env 文件(dotenvy,仅 IM_ENV=dev 时)
3. 读取环境变量(覆盖上面)
4. 解析双密钥 JSON 数组(IM_JWT_SIGNING_KEYS)
5. 解析 server_secrets JSON map(IM_SERVER_SECRETS)
6. 校验必填项,缺失 → log::error + process::exit(78)
7. 构造 AppConfig 注入 Runtime
```

> 实现:`crates/im-common/src/config.rs::AppConfig::load()`(WBS D-1)

## F. 配置热重载(`environments.settings`)

> 引用: `ImplementationSpec §6.3`

| 步骤 | 说明 |
|---|---|
| 1. 启动时全量加载 | 全量加载所有 `environments` 行 → 写入 Valkey(`env:settings:{environment_id}`,TTL 3600s) |
| 2. 内部 API | `POST /v1/internal/environments/{id}/settings`(仅 im-core 内部签名校验) |
| 3. 失效广播 | Valkey pub/sub topic `env:settings:invalidated` 广播失效 |
| 4. 监听重载 | im-core 各实例 background task 监听 topic,收到后**只**重载本实例已缓存的该 env(避免雪崩) |

> 监听器实装位置: `crates/im-core/settings/service.rs::SettingsService::run_invalidation_listener`(per `ImplementationSpec §7.4.6`)

## G. K3s Secret Key 与环境变量映射

| Secret Key | 注入到 | 备注 |
|---|---|---|
| `server_secret` | (经 `IM_SERVER_SECRETS` map 加载) | 仅服务端可读 |
| `jwt_signing_key_v1` | (经 `IM_JWT_SIGNING_KEYS` 数组第 1 项) | |
| `jwt_signing_key_v2` | (同上,轮换期第 2 项) | 轮换结束后 7 天观察期后删除 |
| `refresh_token_pepper` | `IM_REFRESH_TOKEN_PEPPER` | |
| `database_url` | `IM_DATABASE_URL` | 共享(全 env 用同一 PG 集群) |
| `valkey_url` | `IM_VALKEY_URL` | 共享 |
| `nats_url` | `IM_NATS_URL` | 共享 |
| `minio_endpoint` | `IM_MINIO_ENDPOINT` | 共享(V1+) |
| `minio_access_key` | `IM_MINIO_ACCESS_KEY` | 共享(V1+) |
| `minio_secret_key` | `IM_MINIO_SECRET_KEY` | 共享(V1+) |

## H. 密钥轮换流程(6 步,per `ImplementationSpec §6.4` + `BasicDesign §14.4.2`)

```
1. 生成新密钥 → 写入 `im-env-{env_id}-new` Secret(不覆盖旧)
2. 应用同时支持 v1/v2 校验(在 `IM_JWT_SIGNING_KEYS` 中并存)
3. `POST /v1/internal/auth/rotate-start` 通知游戏服务器
4. 观察期 7 天,监控 v1 Token 占比 < 1%
5. 移除 v1 配置,删除旧 Secret
6. 写入 `audit_logs`(action=secret_rotation)
```

> 完整轮换脚本 + 监控待 POC-03 实装(`aux-06 §G`)

## I. 环境差异(dev / staging / prod)

> 引用 `ImplementationSpec §6.3` + `BasicDesign §14.4`

| 维度 | dev | staging | prod |
|---|---|---|---|
| `IM_LOG_LEVEL` | `info,sqlx=warn` | `info,sqlx=warn` | `info,sqlx=warn` |
| `IM_RATE_LIMIT_SEND_MESSAGE_PER_MIN` | 1000(宽松) | 200(中) | 60(严格) |
| `IM_MESSAGE_RECALL_WINDOW_SECONDS` | 3600 (1h) | 600 (10min) | 120 (2min) |
| `IM_MESSAGE_MAX_SIZE_BYTES` | 1MB(大) | 256KB(中) | 64KB(严格) |
| `IM_DB_POOL_MAX_CONNECTIONS` | 5(小) | 20(中) | 50(大) |
| `IM_AUTH_MIDDLEWARE_CACHE_SECONDS` | 0(关闭) | 5 | 5 |
| `IM_PROMETHEUS_BIND` | `0.0.0.0:9100` | `0.0.0.0:9100` | (内网 IP allowlist) |
| `IM_OTEL_EXPORTER_OTLP_ENDPOINT` | (空,关闭) | `http://otel-collector.im1-obs:4317` | `http://otel-collector.im1-obs:4317` |
| `IM_MINIO_*` | mock | staging bucket | prod bucket |
| `IM_NATS_URL` | `nats://nats:4222` | `nats://nats-staging.im1-staging:4222` | `nats://nats-prod.im1-prod:4222` |
| `environments.settings.friend_system_enabled` | true | true | true |
| `environments.settings.voice.enabled` | false | true(测试) | true(灰度) |
| `environments.settings.audit.detailed` | true(开发) | true(合规预演) | true(合规) |

> **派生**: `environments.settings` 各字段在 BasicDesign §14.3 有 default,实际 prod 部署走"环境 toml 文件 + env 覆盖"

## J. 热更新 / 不可热更新分类

| 类型 | 类别 | 重载方式 |
|---|---|---|
| **可热更** | 限流阈值 / Recall 窗 / TTL / 实验开关 | SIGHUP + 定期 poll(30s) + NATS pub/sub 推送(对 `environments.settings`) |
| **不可热更** | 端口 / DB 地址 / 密钥 / Secret URL | 改 config + 重启 pod |

> 多实例下,热更新走 NATS pub/sub 广播,各实例同步(`ImplementationSpec §6.3`)

## K. 校验实现(`AppConfig::load()`,待 D-1 实装)

```rust
// crates/im-common/src/config.rs(待 WBS D-1 实装,token 200K-400K)
use figment::{Figment, providers::{Format, Toml, Env, Json}};
use secrecy::{Secret, ExposeSecret};

#[derive(Debug)]
pub struct AppConfig {
    pub env: Environment,                       // dev / staging / prod
    pub database_url: Secret<String>,          // IM_DATABASE_URL
    pub valkey_url: Secret<String>,            // IM_VALKEY_URL
    pub nats_url: Secret<String>,              // IM_NATS_URL
    pub http_port: u16,                        // IM_HTTP_PORT
    pub grpc_port: u16,                        // IM_GRPC_PORT
    pub jwt_signing_keys: Vec<SigningKey>,     // IM_JWT_SIGNING_KEYS JSON
    pub server_secrets: HashMap<Uuid, Secret<[u8; 32]>>,  // IM_SERVER_SECRETS
    pub refresh_token_pepper: Secret<[u8; 32]>,          // IM_REFRESH_TOKEN_PEPPER
    pub log_level: String,                     // IM_LOG_LEVEL
    pub ws: WsConfig,                          // 3 项 WS 配置
    pub rate_limit: RateLimitConfig,           // 4 项限流
    pub message: MessageConfig,                // 2 项消息
    pub token: TokenConfig,                    // 2 项 Token TTL
    pub db_pool: DbPoolConfig,                 // 1 项连接池
    pub auth: AuthConfig,                      // 1 项缓存
    pub obs: ObsConfig,                        // 3 项可观测
    pub guest_default_name: String,            // IM_GUEST_USER_DEFAULT_NAME
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        // 1. figment: 加载 config/{IM_ENV}.toml + .env(仅 dev)+ env vars
        // 2. 解析 JSON 数组 / map
        // 3. 校验 11 条规则(§D)
        // 4. 失败 → process::exit(78)
        // 5. 成功 → 返回 Self
    }
}
```

## L. 监控 / 告警(配置相关)

| 指标 | 阈值 | 告警 |
|---|---|---|
| 配置加载失败 | 启动时 | 进程退出 + k8s CrashLoopBackOff |
| 必填项缺失 | 启动时 | 同上 |
| 校验失败 | 启动时 | 同上 |
| 热更新失败 | 单次 | 通知 SRE |
| 密钥轮换到期 | 季度前 7 天 | 通知 SRE + PM |
| 未注册配置项使用 | 任何 | 拼写错误告警(避免 `IM_RATELIMIT_*` vs `IM_RATE_LIMIT_*` 错位) |
| K3s Secret 缺失 | 启动时 | 启动失败 + 告警 |
| Secret 即将过期 | 7 天前 | 通知 PM + SRE |

## M. 验收标准 (Acceptance Criteria)

- [ ] §A 27 个环境变量齐全(必填 9 + 可调 15 + 可观测 3)— 对应 `ImplementationSpec §1.1`
- [ ] §B 10 个 K3s Secret Key 齐全(per-env 4 + shared 6)— 对应 `ImplementationSpec §1.1`
- [ ] §C 9 个 `environments.settings` 字段 — 对应 `BasicDesign §14.3`
- [ ] §D 11 条校验规则全部对应 `ImplementationSpec §6.2` 校验表
- [ ] §E 7 步加载顺序 + §F 4 步热重载 + §G 10 条 Secret 映射 + §H 6 步密钥轮换
- [ ] §I 13 项环境差异(dev/staging/prod)与 `BasicDesign §14.4` 一致
- [ ] §J 热更 / 不可热更分类明确
- [ ] §K `AppConfig::load()` Rust trait 草案
- [ ] §L 8 项监控告警

## N. 关联文档 (References)

- 配置源: `docs/ImplementationSpec.md` §1.1(27 + 10 计数)+ §6(完整 6 子节)+ §6.1(加载顺序)+ §6.2(11 校验)+ §6.3(热重载)+ §6.4(轮换)
- 上游: `docs/BasicDesign.md` §14(配置 + 9 项 settings)+ §14.3(9 项 settings)+ §14.4(环境差异)+ §14.4.2(轮换)
- 上游: `docs/DetailedDesign.md` §10(配置项 23 + 4 Secret)
- Cargo 依赖: `ImplementationSpec §2.2`(`figment = 0.10` + `dotenvy = 0.15` + `secrecy = 0.8`)
- 命名规范: `aux-01-naming-convention.md` §D
- 错误码: `aux-03 §B`
- 状态机: `aux-04 §B`(影响热更策略)
- 协议冻结: `ImplementationSpec §3.2.4`
- 部署: `ImplementationSpec §8.4` K3s + GitHub Actions
- 限流: `ImplementationSpec §11.3` + `crates/im-gateway/ratelimit.rs`
- WBS D-1 任务: `132-wbs.md §D-1`(token 200K-400K,figment + dotenvy + 双密钥 JSON)

## O. 已知缺口 (Known Gaps)

| 编号 | 缺口 | 影响 | 跟进 |
|---|---|---|---|
| GAP-1 | §K `AppConfig::load()` 未实装,当前仅设计 | WBS D-1 待实装 | WBS D-1 token 200K-400K |
| GAP-2 | §I 13 项环境差异的部分字段(`IM_DB_POOL_MAX_CONNECTIONS` 各环境)无显式文档 | 部署时易混 | V1+ `config/{env}.toml` 模板 |
| GAP-3 | 密钥轮换 7 天观察期具体监控指标(v1 token 占比)未定义 | 自动判断轮换完成度 | POC-03 实装时定 |
| GAP-4 | `IM_GUEST_USER_DEFAULT_NAME` 是 §A.2 第 15 项,无对应 `ImplementationSpec §1.1` 显式列出 | 实际是隐性需要 | V1+ 评估 |
| GAP-5 | V1+ MinIO 3 项(§B.2 第 35-37)MVP 不需要,但 §A.3 没列 | MVP 阶段 §A 仅 24 项,§B 7 项,实际 27+10=37 项 | V1+ 加 |
| GAP-6 | `IM_OTEL_EXPORTER_OTLP_ENDPOINT`(§A.3 第 26)MVP 默认关闭,V1+ 启用 | 当前不写 OpenTelemetry | V1+ WBS |
| GAP-7 | 未注册的"实验性"配置(如 `IM_FEATURE_*`)无明确分类 | 可能误用为业务开关 | V1+ 治理 |
| GAP-8 | `environments.settings` 9 项中 V1+ 才用 6 项(retention / voice / audit.detailed) | MVP 阶段无需实装 | V1+ |
| GAP-9 | 密钥轮换工具脚本(`scripts/rotate_secret.sh`)V1+ 实装 | MVP 手动轮换 | V1+ |
| GAP-10 | Secret 即将过期提醒(7 天前)无 CI/CD 集成 | 靠人工巡检 | V1+ GitHub Actions 定时 |
| GAP-11 | 拼写错误告警(L 项第 6 条)无 `figment` 自动化检查 | 配置名错位难发现 | V1+ 启动时 `validate_keys` |
| GAP-12 | V1+ 引入 Vault(`ImplementationSpec §11.5` 不做)后,§A.1 第 7-9 项 + §B.1 28-31 需重写 | 当前 K3s Secret 走 IM_ env 注入 | V1+ Vault SDK |

## P. 变更记录 (Change Log)

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | (模板初版) | 初版通用模板 |
| 1.1.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 填实 IM1.0 27 env vars(必填 9 + 可调 15 + 可观测 3) + 10 K3s Secret Key(per-env 4 + shared 6) + 9 environments.settings 字段 = 46 项完整清单;§D 11 校验规则 + §E 7 加载顺序 + §F 4 热重载 + §G 10 Secret 映射 + §H 6 密钥轮换 + §I 13 环境差异 + §J 热更分类 + §K AppConfig::load() Rust trait 草案 + §L 8 监控告警 + §O 12 已知缺口;严格对应 ImplementationSpec §1.1 计数(27+10)+ §6.2 校验表 + BasicDesign §14.3 9 settings;引用 aux-01/03/04/13 + WBS D-1 完整交叉引用 |
