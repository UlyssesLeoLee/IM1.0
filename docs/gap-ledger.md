# 缺口台账 (Gap Ledger)

> **本文档是什么**: 26 处源码注释引用「守门 #1 缺口台账」,但该台账文档
> **此前并不存在** —— 属悬空引用。本文件把**已写在代码注释里的**缺口声明
> 集中成一份可勾选的清单, 使代码注释里的「缺口 #X」可被检索和跟踪。
>
> **本文档不是什么**: 它**不是规范**, 也**不定义** `守门 #1` 的含义。
> 每一行的理由都**摘自对应源码位置的注释原文**, 不含任何本文件新增的判断。
> `守门 #1` 本身仍无定义文档 —— 见 `dangling-references.md`。
>
> 记录于: dev @ `0a42026` (2026-10-03)

---

## 0. 使用方式

- 接线完成后, 删掉代码里的 `#[allow(dead_code)]` 与该处注释, 并在本表勾掉对应行。
- 新增缺口时, 沿用 `#A` `#B` … 的字母编号, **不要复用已勾掉的编号**。
- 代码里引用本表时写「守门 #1 缺口台账: 缺口 #X」, 便于 `grep '缺口 #'` 检索。

---

## 1. im-gateway — 缺口 #A .. #I

| 编号 | 位置 | 缺口内容 (摘自代码注释) | 接线条件 / 依赖 |
|---|---|---|---|
| **#A** | `src/main.rs:56` `DEFAULT_ACCESS_TOKEN_TTL_SECONDS` | `http/auth_handlers.rs` 响应 `expires_in` 当前写死 900 | V1 抽 `TokenService::access_ttl_seconds()` 后改为读本常量 |
| **#B** | `src/http/error_response.rs:26` `app_error_to_response` | 现有 handler 走 `crate::error::error_to_response`, 本重导出未被调用 | error 层收敛后统一走本模块入口 |
| **#C** | `src/http/auth_handlers.rs:203` `GuestRegisterRequest.device_fingerprint` | task 规范要求的可选 fingerprint, 字段在但没人读 | `IdentityService::guest_register` 支持 fingerprint 入参后写入 |
| **#D** | `src/ws/heartbeat.rs:27` `HeartbeatConfig.expected_ping_interval` | C-11 driver 接线后用于检测 ping 迟到; 当前 tick 只判 `no_frame_timeout`, 不消费本字段 | C-11 driver 接线 |
| **#E** | `src/ws/heartbeat.rs:77` `remaining()` / `config()` | 供 driver 在 tick 循环设 wakeup 间隔 / 读超时配置做日志上报; 当前 driver 走固定 30s tick | C-11 driver 接线 |
| **#F** | `src/ws/heartbeat.rs` `PingFrame` / `PongFrame` / `PingPongType` / `into_pong` | C-12 预留的 wire 镜像 | ✅ **二选一已定(2026-10-03)**: 走 `im_protocol::ws_frames`。`handler.rs` 的 Ping 分支用 `ClientFrame::Ping` + `ServerFrame::Pong`, 故这批定义成为未接线的重复实现, 保留不删(§1.1.1) |
| **#G** | `src/ws/session.rs:95/102` `user_id()` / `environment_id()` | 鉴权后身份读取接口; 另有「同 user_id 多租户路由」 | C-11 driver 做 ForceDisconnect / 广播分发时按 user_id 定位 |
| **#H** | `src/ws/session.rs:104` `tick_heartbeat()` | 心跳 tick 返回 true 表示应主动 close | ✅ **已结清** (2026-10-03, `d89b88d`): 调用点搬进 `run_ws_loop` 的 `select!`, 超时真正关闭连接 |
| **#I** | `src/ws/session.rs` `WsInbound` / `handle_ping()` / `From<ClientFrame>` | C-11 driver 统一入站分派入口 | 入站分派**已存在**于主循环 `match` 并已补 Ping 分支; 但走 `ClientFrame` 而非本枚举, 故本枚举成为第二套未接线抽象, 清理时与 #F 一并处理 |

> #D/#E/#F/**#G**/#H/**#I** 中, **#H 已于 2026-10-03 结清**; 余下 #D/#E/#F/#G/#I
> 仍全部阻塞在 C-11 (WsSession + actix-ws driver 实装)。换句话说这 5 个剩余缺口
> 是同一个上游任务的下游, 接线 C-11 可一次性清掉。

### 1.1 #H 的严重性升级: 60s 无帧超时**根本不生效** (2026-10-03 复核 → 同日已修)

> **✅ 已于 2026-10-03 修复(`d89b88d`)**。本节保留原始发现过程, 作为
> 「文档声称的行为没实现」的实例。以下是**修复前**的代码。

复核 `src/ws/handler.rs:113-147` 时发现, #H 不是一个"方法没被调用"的无害缺口,
而是**行为与文档不符的功能漏洞**:

```rust
// 后台任务 (line 115-125)
tokio::spawn(async move {
    let mut tick = interval(Duration::from_secs(30));
    loop {
        tick.tick().await;
        if hb_for_tick.tick() {
            tracing::info!(... "heartbeat timeout, closing ws");
            // 注: session 关闭由主 loop 检测; 此处仅日志, 实际关闭在主 loop
            break;
        }
    }
});

// 主循环 (line 153-158)
async fn run_ws_loop(ws_session: &mut actix_ws::Session, msg_stream: &mut actix_ws::MessageStream, ...) {
    while let Some(msg_result) = msg_stream.next().await {   // ← 只等消息, 没有 select 超时信号
```

问题: 超时发生后后台任务只 `break` + 打日志, 而
- 主循环只 `await msg_stream.next()`, **没有 `select!` 任何超时信号**;
- `actix_ws::Session` 归主循环所有, 后台任务拿不到, 无法主动 close。

**实际后果**: 客户端停止发帧后, 连接**不会被 60s 超时关闭**, 而是无限期保持。
而 `handler.rs:16` 的模块文档写的是「30s background task tick_heartbeat +
60s 无帧超时关闭」—— **文档描述的行为没有实现**。半开连接会一直堆积直到
TCP 层超时, 属资源泄漏。

**修法**(已实施, `d89b88d`): 采用上面两个方向中的第二个 —— 把 `interval` 直接搬进
`run_ws_loop`, 用 `tokio::select!` 同时等 `msg_stream.next()` 与 tick; 判超时即
`force_close()` + return, 由 `ws_handler` 统一走 close handshake。原先那个后台
task 已删除。`select!` 用 `biased` 让帧优先 —— 客户端持续发帧时 tick 分支不命中是
正确的(每帧都重置 idle), 只有停发帧时才会落到 tick 分支。

**新增 2 个回归测试**(`im-gateway --bins` 51 passed / 0 failed)锁住该判定。

**仍未覆盖的边界(如实记录)**: 测试验证的是「tick 驱动下超时判定正确」, **不是**
「连接真的被关闭」。后者需要真实 WS 端到端, 而本仓库当前没有任何 WS 客户端依赖
(无 `awc` / `tokio-tungstenite`), 且 CI 用 `--locked` 不宜临时加依赖。若要把这条
端到端补上, 前置条件是: 引入 WS 客户端 dev-dependency + 起 actix test server +
构造可用的 `AppState`(需 PG)。

**教训(第四次同形)**: 这条 bug 的成因与我三轮前「用 `tokio::time::pause()` 测
`std::time::Instant` 计时」的失败是同一类 —— **我控制的量与被测对象读的不是同一
个**。写验证前先问「我推进的时钟 / 解析的范围 / 判断的环境, 是不是生产代码实际
用的那一套」。

### 1.2 同类第二例: ping 不回 pong (2026-10-03 发现并已修)

修完 #H 后顺手复核 WS 入站分派, 立刻发现**同一形状的第二个功能漏洞**:

`run_ws_loop` 鉴权后的分派原本只有两个分支 ——

```rust
match serde_json::from_str::<ClientFrame>(text_str) {
    Ok(ClientFrame::Auth { .. }) => { /* 拒绝重复 auth */ }
    Ok(_) => { /* 业务帧兜底: 返回 UNSUPPORTED_OPERATION */ }   // ← Ping 落这里
    Err(_) => { /* invalid frame json */ }
}
```

`ClientFrame::Ping` **是存在的独立变体**(见 `im-protocol/src/ws_frames.rs`),
但没有对应分支, 于是被 `Ok(_)` 兜底吞掉, 回 `UNSUPPORTED_OPERATION`。
**结果: 客户端发 ping 永远收不到 pong**, 违反 aux-13 §1.1.8。

这也是 #F 里 `PingFrame::into_pong` 一直无人调用的**直接原因** —— 不是忘了接线,
是接线点从来没被执行到。

**修法**: 补 `Ok(ClientFrame::Ping { ts })` 分支, 回
`ServerFrame::Pong { ts: ts.unwrap_or(0) }` —— 与该分支族已有的 `ClientFrame`
同源, 顺带把 #F 的二选一决策落地为「走 im_protocol」。

**新增 5 个 wire 契约测试**(纯 serde 层, 无需 WS 连接): ping 能解析成
`Ping` 变体、缺省 ts 解析为 None、pong 的 type/ts 形状、缺省 ts 回 0 而非省略
字段、以及显式锁住「ping 不会被误判成业务帧」。

**共性**: #H 与本例都是「模块文档描述了一条从未真正执行的行为」。这类缺陷不会
让编译或任何现有测试失败 —— 它们只会在真实客户端按协议发帧时才暴露。故本次
一并把 WS 分派的每个分支对照 `im_protocol::ws_frames` 核过一遍。

### 1.3 严重安全漏洞: token_exchange 端点**无任何签名验证** (2026-10-03 发现并已修)

**这是至今发现的最严重问题, 严重性高于前两个 WS 功能缺陷 —— 前两个导致资源泄漏,
这个导致凭据伪造。**

#### 缺陷

`POST /v1/auth/token/exchange` **已注册路由**(`http/mod.rs` L31-34), 而
`http/mod.rs` L7 的注释还声称「MVP Day 4: C-3..C-7 wired」。但实际上:

- `http/auth.rs` L11 明确写着「C-3 (token_exchange + **HMAC 签名校验**) 留给
  auth-lane worker」—— **该中间件从未实现**
- `auth_handlers.rs` 把 `server_signature_verified` **硬编码为 `true`**, 注释
  假定「HMAC 在中间件层已校验」
- `IdentityService::server_secrets` 字段收了 `main.rs` 传入的
  `HashMap<EnvironmentId, SecretString>`, 但**从未被读取**
- `server_exchange_token` 只信任调用方传的布尔值, 内部无任何签名校验

**后果**: 任何能访问该端点的人, POST 任意
`{environment_id, external_provider, external_uid}` 即可换到**该 external_uid
对应账号的 access token** —— 即冒充任意游戏账号。`server_exchange_token` 里那句
`if !cmd.server_signature_verified { return Unauthorized }` 的保护形同虚设。

#### 修法

协议早已定义(aux-13 §3.1), 依赖也早已在 workspace 里(`hmac`/`sha2`/`hex`),
`main.rs` 也已把 secret 传进 `IdentityService` —— **唯一缺的是验证逻辑本身**:

1. `IdentityService::verify_server_signature(env, raw_body, sig, ts, now)`:
   - 缺 signature / 缺 timestamp → `Unauthorized`
   - `|now - ts| > 300s` → `Unauthorized`(压重放窗口到 10 分钟)
   - 该 environment 未配 secret → `Unauthorized`(**fail-closed**)
   - `hex(HMAC-SHA256(secret, raw_body))` 用**恒定时间比较**(新加
     `common::crypto::constant_time_eq`, 防时序攻击)
2. handler 签名从 `web::Json<TokenExchangeRequest>` 改为 `web::Bytes` + `HttpRequest`
   —— 签名基于**原始字节**, `web::Json` 会消费并丢弃原始 body, 无法复原
3. 验签通过后才设 `server_signature_verified: true` —— 此时的 `true` 是验签结果,
   不是假设

#### 新增测试(9 个)

`identity/tests.rs`: 正确签名通过 / 缺签名拒 / 错签名拒 / **body 被篡改后原签名失效**
/ 过期时间戳拒 / 缺时间戳拒 / 未配 secret 拒(fail-closed)/ **A 环境签名不能用于
B 环境**; `crypto.rs`: 恒定时间比较与普通 `==` 行为一致。

#### 仍然未做: nonce 防重放

协议有 `X-IM-Nonce`, 但服务端既不校验也不记录 —— 同一合法请求可在 ±300s 窗口内
**重放**。完整防重放需跨实例共享存储 → 依赖 **WBS D-4 (Valkey)**。已记入 §2 缺口表。
时间戳窗口只能把重放窗口压到 10 分钟, **不能消除**。

**教训**: 这个漏洞的成因是「注释里写了一个不存在的前置条件(中间件已校验), 而后来者
读到注释就相信了它」。它和 §1.1/§1.2 的两个 WS 缺陷是同一族 —— **注释/文档里的
断言必须由代码验证, 不能由注释自证**。三处都是「文档声称有, 代码里没有」。

### 1.4 §1.3 的验证盲区: 只测 service 层证明不了 handler 调了验签 (2026-10-03)

§1.3 修完后补的 9 个测试**全部落在 service 层**(`IdentityService::verify_server_signature`)。
但原漏洞的形态是「**handler 压根没调用**它」—— service 层测得再全, 也证明不了
handler 接了线。handler 可以明天再次忘记调用, 而 9 个测试全绿。

补 2 个 **handler 层端到端测试**(`auth_handlers.rs` 的 `mod tests`), 用
`actix_web::test::init_service` 发**真实 HTTP 请求**:

- `token_exchange_without_valid_signature_returns_401_and_creates_nothing` ——
  无签名 / 错签名 / 过期时间戳三种请求均 401, 且断言
  `users` 表中该 `external_uid` 计数为 **0**(验签失败不得留下任何痕迹)
- `token_exchange_with_valid_signature_returns_200` —— 正确签名换出非空
  `access_token` + `refresh_token`, 且库中恰好 1 个 user

因 `AppState.identity_service` 的类型写死为
`IdentityService<PgUserRepository, PgDeviceSessionRepository>`, 这 2 个用例**必须连
真 PG**; 未设 `DATABASE_URL` 时跳过(CI 无 PG)。

#### 变异测试: 这 2 个测试确实抓得住回归(不能只看它变绿)

「绿」本身不是证据 —— 跳过分支也可能让测试空跑通过。故把 handler 的验签调用
**整段摘掉**(等于把漏洞装回去)再跑:

| 用例 | 摘掉验签后 | 结论 |
|---|---|---|
| `..._401_and_creates_nothing` | **FAILED** — `left: 200, right: 401` | 无签名请求真的换出了 token, 与原漏洞现象一致 |
| `..._valid_signature_returns_200` | **FAILED** — `left: 2, right: 1` | 确实打到了真 PG 并读到了真实行数 |

两个失败互为佐证: 后者能报出**具体的库内计数**, 顺带证明这组用例不是空跑。

#### 变异过程中发现并修掉的测试自身缺陷

第二个用例原本把 `external_uid` 写死为 `e2e-valid-uid`。变异跑完还原后再跑一次,
它就会因为**上一轮已落库**而报 `count == 2` 失败 —— 即该用例**不可重入**,
在持久化 DB 上第二次跑必然假失败。已改为每次运行拼 `Uuid::new_v4()` 前缀,
并连跑两次验证 `ok, ok`。

**教训(与本文件其余条目同源)**: 「测试通过」和「测试有判别力」是两件事。
判别力只能靠**故意注入缺陷**来证明, 不能靠绿灯推断; 而注入缺陷的过程本身
会暴露测试自身的不可重入等缺陷。

### 1.5 C-7 logout 是死路: access token 无法被吊销 (2026-10-03 发现并已修)

#### 缺陷

`POST /v1/auth/logout` **已注册路由**(`http/mod.rs`), 但 handler 无条件返回错误:

```rust
let _ = app; // 占位 — 实际需要解 dsid
Err(json_response(ErrorCode::ValidationError, None, Some("C-7 logout: ... 未实装")))
```

**后果: 泄露的 access token 在有效期内无法被吊销** —— 服务端没有任何途径
让一个 session 失效。这是**安全能力缺失**, 不是「功能没做」。

#### 三处说法互相矛盾(比缺陷本身更值得记)

同一个事实, 三处的描述各不相同:

| 位置 | 说法 |
|---|---|
| `http/mod.rs` 路由注释 | 「device_session_id JWT claim 未实装, **兜底 401**」 |
| handler 函数文档注释 | 「兜底: 返 **501** Not Implemented」 |
| handler 实际代码 | 返 **400** `ValidationError` |

**没有一处说对。** 读注释的人会以为「至少状态码是准确的」, 而实际行为与
全部三处描述都不符。

#### 根因: 签发顺序反了

`IdentityService::issue_token_pair` 原本是:

1. `issue_access_token(&user)` —— 先签 access token
2. `device_repo.create(...)` —— 后建 device session

但 `dsid` claim 要写进 access token, **签名时 session id 还不存在**。
所以不是「忘了加 claim」这么简单, 是**顺序必须重排**才能加。

修法(全部三处):

1. `TokenClaims` 新增 `dsid: Option<String>`
   (`#[serde(default, skip_serializing_if = "Option::is_none")]`)
2. `TokenService::issue_access_token_for_session(user, Option<DeviceSessionId>)`;
   原 `issue_access_token(user)` 保留为 `dsid=None` 的薄封装(不改动既有调用点)
3. `issue_token_pair` **先建 session 再签 token**
4. `AuthedUser` 新增 `device_session_id`, 由 extractor 从 claims 填充
5. `logout` 真正调 `IdentityService::logout` → `device_repo.revoke` → 返 204

#### 顺带更正: C-6 的路由注释是错的

`http/mod.rs` 写「C-6 link_account 待 UPDATE 实现, 当前返 InternalError」——
**不成立**。`IdentityService::link_account` 早已实装完整的 5 步
guest→user 升级流程(验 token / 校验 kind='guest' / 唯一性由
`uniq_users_env_extid` 兜底 / UPDATE / 重签 TokenPair)。已更正该注释。

#### 旧 token 的降级行为

本字段加入前签发的 token 没有 `dsid`, 解析为 `None`, 此时 logout 返 **401**
而非静默 204 —— 返 204 会让客户端以为已登出而服务端什么都没做。
客户端须重新登录换一枚带 `dsid` 的 token。

#### 新增测试(5 个)

`token.rs`(3 个, 纯 serde 层): `dsid` 往返 / 无 session 时 `dsid` 为 `None`
而非空串(供 logout 区分新旧 token) / 旧 token 不含该 JSON 字段但仍能解析。
`auth_handlers.rs`(2 个, 真 PG 端到端):

- `logout_revokes_the_device_session_in_db` —— 登录 → 登出 → 断言
  **`device_sessions.revoked_at` 真的被写入**。关键断言不是状态码, 而是
  服务端状态真的变了(同源原则)
- `logout_without_dsid_claim_returns_401` —— 缺 `dsid` 返 401 而非假装成功

**变异测试**: 把 `logout` 改成「返 204 但不调 revoke」, 该用例在
「logout 必须真吊销」断言上 **FAILED** —— 证明它测的是状态变化而非状态码。

**共性**: 本例与 §1.1/§1.2/§1.3 同族 —— **文档/注释描述了一个不存在的
实现**。不同之处在于前三处是「文档说有, 代码没有」, 本处是「文档说有,
代码有一个永远失败的桩, 而文档对它的失败方式描述得都不一样」。



---

## 2. 后续新增 (无字母编号, 2026-10-03 标注时未分配编号)

| 位置 | 缺口内容 (摘自代码注释) | 接线条件 / 依赖 |
|---|---|---|
| `crates/im-gateway/src/health.rs:15` `readyz()` | F-4 (healthz/readyz) 路由尚未接线 | 依赖 **F-2 / F-3 (K3s 部署)**, 二者受 F-1 (Docker daemon 间歇性故障) 阻塞 |
| `crates/im-gateway/src/http/state.rs:71` `AuthedUser.tenant_id` | 已聚合进鉴权上下文, 但现有 handler 尚未按租户过滤 | 多租户隔离随 **G-1 / V1** 落地 |
| ~~`crates/im-core/src/identity/service.rs` `server_secrets`~~ | ~~S2S token exchange 要按 environment 取 secret, 接线未完成~~ | ✅ **已接线** (2026-10-03): 被 `IdentityService::verify_server_signature` 真正使用, 见 §1.3 |
| `crates/im-gateway/src/http/auth_handlers.rs` `token_exchange` **nonce 防重放** | 协议有 `X-IM-Nonce`, 但服务端**未校验也未记录** —— 同一合法请求可在 ±300s 窗口内重放 | 需跨实例共享存储 → **WBS D-4 (Valkey)** 落地后接 |
| `crates/im-gateway/src/placeholder.rs` (整文件) | 10 个端点桩函数未被调用 | 该文件唯一职责就是存放未接线桩; 各端点随对应 WBS 项落地 |

---

## 3. lint 压制相关 (非功能缺口)

| 位置 | 内容 | 待办 |
|---|---|---|
| `crates/im-proto/src/lib.rs:7` `#![allow(clippy::all)]` | 全仓唯一保留的 blanket allow。唯一手写代码只是一层 `include_proto!` 转发, 实质内容全由 tonic-build 生成 (`OUT_DIR/im.core.v1.rs`), 生成物稳定触发 `result_large_err` 等 lint (2026-10-03 实测 22 处) 且不受我们控制 | 收窄到 `#[allow(...)] pub mod generated { include_proto!(...) }`, 使手写部分重新受 clippy 管辖 |

> 此前 17 处 blanket allow 已于 2026-10-03 全部移除 (commit `45dcb72`),
> 移除后 `cargo clippy --workspace --all-targets -- -D warnings` 仍为 exit 0。

---

## 4. 依赖链一览

```
C-11 (WsSession driver 实装)
   └─▶ #D  #E  #F  #G  #H  #I        (6 个, 可一次性清)

C-3  (S2S token exchange 接线)
   └─▶ server_secrets                (1 个)

F-1  (Docker daemon, 间歇性故障)
   └─▶ F-2 (K3s dev namespace)
         └─▶ F-3 (CI deploy-dev)
               ├─▶ F-4 (healthz/readyz 路由)  ──▶ readyz
               └─▶ E-3 / G-* 等依赖部署的项

G-1 / V1 (多租户隔离)
   └─▶ AuthedUser.tenant_id
```

---

## 5. 不做的事 (禁止猜测)

- ❌ **不定义 `守门 #1` 的含义** —— 它在本仓库无定义文档, 本表只汇总各次会话
  在代码注释里的**局部用法**, 不构成该编号的正式定义。
- ❌ **不因本表存在就认为缺口已登记完毕** —— 本表只覆盖**代码注释里已写明**的
  缺口; WBS (`docs/132-wbs.md`) 里大量 `Todo` 项并没有对应的 `#[allow]` 或注释,
  不在本表范围内。
- ❌ **不代替 WBS 排期** —— 优先级与工时仍以 `docs/132-wbs.md` 为准。

---

**维护**: 本表为快照, 记录于 dev @ `0a42026` (2026-10-03)。
缺口被接线后请同步勾除本文档与代码注释两侧, 避免再次出现
「代码引用台账但台账不存在」或「台账有项但代码已删」的双向漂移。
