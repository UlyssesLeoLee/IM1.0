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

### 1.6 伪造的错误码与出处: `UNSUPPORTED_OPERATION` / 501 (2026-10-03 消除)

#### 缺陷

`crates/im-gateway/src/ws/handler.rs` 共有 **4 处**注释声称业务帧
(SendMessage / Edit / Recall / React / MarkRead / Typing) 返回
`UNSUPPORTED_OPERATION`, 并注明 **501, per aux-13 §4**。**两处都是假的**:

| 声称 | 事实 |
|---|---|
| `UNSUPPORTED_OPERATION` 是错误码 | **不存在**。`aux-03-error-code-registry.md` §B 是 MVP 错误码的**唯一权威表**, 列出 21 个已注册码, 与 `im_common::ErrorCode` 枚举逐项一致, 其中没有它, 也没有 501 |
| 出处是 `aux-13 §4` | `aux-13` 的 §4 是**「前置条件 (Prerequisites / Inputs)」**, 不是错误码章节 |

实际代码返回 `VALIDATION_ERROR` (400) —— 是个**已注册**码, 但语义不合:
业务帧格式完全合法, 缺的只是服务端处理器。aux-03 §B 对该码的定义是
「请求体校验失败 / 字段类型、长度、枚举值不合法」。

#### 为何不改成 501

`aux-03 §B` 明文规定「任何 PR 增加必须同时更新本表与 DetailedDesign」——
即**新增错误码是协议变更**, 而 `ImplementationSpec` 处于 `[PROTOCOL-FROZEN]`。
是否新增「未实装」类错误码属**规范所有者**的决定, 不是架构师可自行拍板的
实现细节。故本轮**只更正注释、不动 wire 行为**, 并把决策点显式记下来。

#### 结构性防护: 补上那个从未存在过的检查

追查时发现 `error.rs` 声称「CI 由 `scripts/check_error_codes.sh` 扫描所有
错误码字符串与枚举一致性」—— **该脚本从不存在**。更值得注意的是
`ci.yml:136-137` 早在 2026-10-02 就记录了此事并把该步骤降级为**恒通过的
echo 占位符**, 但 `error.rs` 的声明一直没跟着改。

已新增 `scripts/check-error-codes.ps1` 并挂进 CI `sast` job(替换占位符),
三项**精确匹配、零误报**的检查:

1. `ErrorCode` 枚举变体 ↔ `as_str()` wire 名一一对应且唯一
2. **`aux-03 §B` 权威表 ↔ 枚举 wire 名集双向完全相等** ← 这条正是能抓住
   本次伪造码的检查, 同时也堵住反向缺口(枚举加了码却没更新权威表)
3. 全仓 `crates/**/*.rs` 的 `ErrorCode::X` 用法均指向真实变体

**变异测试**: 把 `EnvironmentDisabled` 的 wire 名改成 `ENVIRONMENT_DISABLED_MUTANT`,
脚本**双向各报一条**并 exit 1; 还原后 exit 0, `git diff` 无残留。

**脚本跨平台**: CI 跑在 ubuntu, 路径一律用正斜杠(Windows 下同样接受);
已在 Windows PowerShell 5.1 与 pwsh 7 双版本下复验, 并按仓库既有要求加
UTF-8 BOM、通过 `scripts/lint-ps1-encoding.ps1`。

#### 明确未自动化的一项

首版脚本还想扫描「注释里提到的错误码是否已注册」, 实测**误报 228 处**
(`IM_HTTP_PORT` / `MAX_PASSWORD_LEN` / `CARGO_PKG_VERSION` / 测试 fixture …)——
**没有语法位置**能区分「散文里提到的错误码」与「环境变量名或常量名」,
该启发式不可用。已从脚本中移除, 并在脚本头部与本文档同时注明**保留人工
review**, 不假装已覆盖。

### 1.7 WS 清扫: 凭空造的 device_session_id + 业务帧丢失 req_id (2026-10-03)

顺着 §1.6 继续清 `ws/handler.rs`, 又发现三个问题。

#### 1.7.1 `handle_auth` 凭空生成 device_session_id (安全)

```rust
// 注释: 「当前用 nil placeholder」
let device_session_id = DeviceSessionId::new();   // 实际: 随机 UUID
```

`device_sessions` 表里**没有**这一行。该值经 `mark_authenticated` 存进
`WsSession.device_session_id`, 于是每个 WS 会话都持有一个**永远匹配不上任何
真实会话**的 id —— 将来任何「按 device session 强制下线」的逻辑都会指向一个
不存在的对象, **而且不会报错**。

**注释比代码更危险**: 写的是「nil placeholder」, 而代码生成的是随机 UUID。
`Uuid::nil()` 一眼看出是假的; 随机 UUID 看着完全正常。**一个占位实现如果
返回了「看起来真的」的值, 就会把未实装伪装成已实装。**

修法: 读 `dsid` claim(§1.5 刚加的)。缺失时**拒绝**而非编造 —— WS 是长连接,
一个无法被吊销的鉴权通道正是 §1.5 关掉的那个缺口。缺 dsid 的 token 是该字段
加入前签发的, 有效期 ≤15min, 重新登录即可。

#### 1.7.2 业务帧失败响应丢失 req_id (协议)

6 类业务帧(`SendMessage` / `EditMessage` / `RecallMessage` / `React` /
`MarkRead` / `Typing`)走的是 `Ok(_) => send_error(..., None)` —— **不传
req_id**。aux-13 §1.2.4 规定 error 帧带 `req_id`, 客户端据此把失败响应关联
回自己的请求; 不传的话, 同一连接上并发多个请求时**客户端无法知道是哪一个
失败了**。

**顺带把 `Ok(_)` 换成显式 or-pattern**(6 个变体全列出), 两个收益:

1. 能取到 `req_id` 并回传
2. match 变**穷尽** —— 未来给 `ClientFrame` 加变体会在此**编译报错**,
   而不会被静默吞掉。ping 漏洞(§1.2)就是这么藏的: `ClientFrame::Ping`
   解析完全成功, 却落进了 `Ok(_)`。

#### 1.7.3 `ServerFrame::Ack` 表达不了「失败」(协议) —— 已修

aux-13 §1.2.4 规定失败响应形状为

```json
{"type":"ack","req_id":"...","ok":false,"error":{"code":"...","message":"...","trace_id":"..."}}
```

但 `im_protocol::ServerFrame::Ack` 原本只有 `{ req_id, ok, data }` —— **有
`ok: bool` 却没有 error 载荷**, 根本表达不了这个形状。这正是当初另造一个
顶层 `{"type":"error", ...}` 帧类型(`WsErrorFrame`)的原因: 实现偏离了自己的
规范形状。

**一度判成「需规范所有者拍板, 不擅改」—— 那个判断下重了, 已被后续证据推翻。**

推翻它的证据在 `im-testkit/src/mock_ws_frames.rs`: 该文件**已经在发规范形状**
(`ack_error_json()` 返回 `{"type":"ack", req_id, ok:false, error:{code,message,trace_id}}`),
而它的文件头明写「**强依赖 im-protocol, 避免另建平行结构**」。也就是说:

1. 规范要求这个形状
2. 项目自己的 testkit 已经在按这个形状造数据
3. **只有** `im-protocol` 的类型表达不了 —— 它是三者中唯一偏离的

这不是「要不要改协议」, 而是**实现去对齐自己的规范和自己的 testkit**,
方向无歧义。`ImplementationSpec` 的 `[PROTOCOL-FROZEN]` 在这里不构成阻碍:
冻结的是协议内容, 而这次改动是让实现回到被冻结的内容。

修法:

1. `ServerFrame::Ack` 新增 `error: Option<ErrorBody>`
   (`#[serde(default, skip_serializing_if = "Option::is_none")]` —— 旧成功
   ack 仍可解析)
2. im-gateway `send_error` 改发 `ServerFrame::Ack { ok:false, error:Some(..) }`,
   **删除** `WsErrorFrame` struct。客户端不必再同时处理 `error` 与 `ack(ok=false)`
   两种错误帧
3. im-testkit 补齐缺失的强类型 `ack_error_frame()`, 消除「JSON Value 兜底
   强类型」的平行结构
4. 构造逻辑抽成纯函数 `error_ack_frame()`, **线上与测试共用同一条路径** ——
   否则测试会验证一个线上并不产生的形状, 那正是本轮反复在堵的那类错

`req_id = None`(非法 JSON / binary 帧 / continuation 帧等协议级错误,
解析不出请求)统一用 `Uuid::nil()` 占位, 表示「无可关联的请求」。

**编译器替我们抓到 3 处构造点**(im-testkit 1、im-protocol 测试 1、以及
编译期即暴露的 `ServerFrame::Ack` 初始化), 逐处补齐 —— 字段变更的真实影响
范围由编译器给出, 不靠估计。


#### 新增测试 (3 个)

| 测试 | 锁住的事实 |
|---|---|
| `ws_auth_returns_the_dsid_carried_by_the_token` | 返回的必须是 **token 里那个** dsid, 不是某个合法 UUID |
| `ws_auth_rejects_token_without_dsid` | 缺 dsid **拒绝**放行, 不编造 |
| `business_frame_error_frame_carries_req_id` | error 帧回传 req_id |

**变异测试(两轮, 均确认有判别力)**:

1. `unwrap_or_else(DeviceSessionId::new)`(仅 dsid 缺失时才造)→
   只有 `ws_auth_rejects_token_without_dsid` 变红
2. `let device_session_id = DeviceSessionId::new();`(原 bug 形态, 总是造)→
   **两个都变红**, 失败信息直接打出两个不同 UUID
   (`left: DeviceSessionId(f8038613-…)` vs `right: DeviceSessionId(037559d9-…)`),
   证明它比的是**真实值**而不是「有没有 id」

**共性**: 1.7.1 与 §1.2(ping 漏洞)是同一处藏东西的形态 ——
**`Ok(_)` 兜底与「返回看似合理的假值」都是把未实装伪装成已实装的手段**,
且两者都不会让任何现有测试失败。

### 1.8 C-9 `send_message` 实装 + `auth_ok` 幽灵帧 (2026-10-03)

#### 1.8.1 6 类业务帧中的第 1 类实装

`SendMessage` 此前落进 `Ok(_)` 兜底回「未实装」错误。现实装, 且**刻意与 REST
`http::messages::send_message` 走同一条 `MessageService::send_message` 校验链**
(幂等 / content schema / conversation member / 大小上限), 差别只在响应形状:
REST 返 201 + body, WS 返 `ServerFrame::Ack { ok: true, data }`。

- **sender 来自已鉴权的会话状态**(`state.user_id()`), 不取自帧内容 ——
  客户端没有资格声明自己是谁
- 幂等: 新增 `MessageService::find_by_idempotency_key` 透传, 因为
  `send_message` 内部遇到重放会返回既有 message 但**不告诉调用方这是重放**,
  而 WS 必须回填 `AckData.idempotent_replay`(per aux-13 §1.2.2/§1.2.3)。
  已知偏差: 并发下两边都查不到时, 由 `send_message` 内部 + 唯一索引兜底, 此时
  重放会被报成 `idempotent_replay: false` —— 保守偏差(把重放报成新消息好过
  反过来), 已在该方法文档注明
- `map_service_error` **不自己维护错误码映射**, 直接用 `AppError::code()`
  (im-common 里的穷尽 match, 单一真源)。若另写一份, 两处会随变体新增而漂移。
  它只自行决定**哪些错误的消息可外泄**: `InternalError` / `ServiceUnavailable`
  只回通用文案(原因可能含 SQL / 连接串 / 凭据), 业务码原样回传
  (aux-03 §B 要求 `VALIDATION_ERROR` 供客户端展示字段级原因)

**仍未实装 4 类**: `RecallMessage` / `React` / `MarkRead` / `Typing`。
`EditMessage` 同日实装(见 §1.9)。

#### 1.8.2 顺带发现: `auth_ok` 是 aux-13 里不存在的帧

鉴权成功后本 handler 回:

```rust
let ack = serde_json::json!({ "type": "auth_ok", "req_id": req_id });
```

`auth_ok` **不是 aux-13 §1.2 定义的任何帧类型**。§1.2.1 定义的是
`connected { session_id }`。这是继 `UNSUPPORTED_OPERATION`(§1.6)之后
**第二处凭空发明的 wire 帧**。

与 §1.7.3 的 `ack` 错误帧形状不同 —— 那次是实现偏离了规范且项目自己的 testkit
已在用规范形状(证据充分, 方向无歧义); 这次**没有任何证据表明 `connected` 就是
原意**, 无法排除当初是有意设计。故**只记录不擅改**(改它会变动客户端可见的
wire 形状), 已记入 §2。

**共性**: §1.6 / §1.8.2 都是「注释与代码自称实现了一个规范里没有的东西」。
规范与实现的偏离在**两个方向**都发生过: 规范有而实现没有(§1.3 C-3 验签)、
实现有而规范没有(本条 `auth_ok`、§1.6 的 `UNSUPPORTED_OPERATION`)。
**双向都需要有人守** —— 只查「代码缺什么」会漏掉「代码多出什么」。

### 1.9 `edit_message`: 做完 4 步校验后无条件返 `InternalError` (2026-10-03 已修)

#### 缺陷

```rust
// 1. 查原 message   ✓
// 2. 仅 sender      ✓
// 3. 大小校验        ✓
// 4. content schema ✓
// 5. 更新(留待 C-9 完整实装)
Err(AppError::Internal(anyhow::anyhow!("edit_message UPDATE not yet implemented in MVP")))
```

**无任何成功路径**, 且返回 `INTERNAL_ERROR` —— 客户端开发者看到 500 会以为服务端
坏了, 而不是「这个功能还没做」。

**这是本文件迄今最隐蔽的一处伪装**: 代码**读起来像只差最后一步 UPDATE**,
而那一步根本不存在。四步完整校验会让人下意识认为「差不多了」—— 相比之下,
`Ok(_)` 兜底(§1.7.1)反倒一眼能看出是没实现的。

#### 三层同时未实装

| 层 | 此前状态 |
|---|---|
| WS handler | `EditMessage` 落进 `Ok(_)` 兜底 -> 回「未实装」 |
| service | 4 步校验后无条件 `InternalError` |
| repository | **根本没有改 content 的方法**(只有 `update_state`) |

而 **schema 一直是齐的**(`messages.content JSONB` + `messages.edited_at
TIMESTAMPTZ`, 见 `0005_create_messages_reactions.sql`)。缺的只是这一条 SQL。

#### 修法

1. `MessageRepository::update_content` —— `UPDATE ... RETURNING` 一次往返拿到
   更新后的行, 免掉「UPDATE 再 SELECT」的第二趟, 也避免返回值与并发写不一致
2. `MessageService::edit_message` 第 5 步真正落库
3. **补状态机守卫(此前完全没有)**: 已 `recalled` / `deleted` 的消息不可编辑。
   此前只看 sender, **撤回的消息仍能被编辑出内容** —— 与「撤回后应只剩墓碑」
   的语义直接冲突。用已注册码 `INVALID_STATE_TRANSITION`(per aux-03 §B, 409),
   不新造码
4. `MessageState::as_str()` 提到枚举自身(原先是 pg.rs 里的私有函数, service
   层拿不到, 只能在错误信息里拼字面量)—— 消除重复映射
5. WS handler 侧接线 `EditMessage`(§1.8.1 的 `2/6`)

仓储层**故意不加** `WHERE state NOT IN ('recalled','deleted')`: 状态机判定是
service 层职责(它已持有旧行、知道状态), 两处都判会重复, 且仓储层无权决定
业务规则。

#### 测试(4 个, 真 PG) + 变异

`edit_updates_content_and_stamps_edited_at`(成功后**直接查库**确认 content 与
edited_at 真落盘, 不只信返回值) / `edit_rejected_for_non_sender` /
`edit_rejected_for_recalled_message`(并断言**被拒的编辑没改动内容**) /
`edit_missing_message_returns_not_found`。

**变异测试**: 把第 5 步退回旧的无条件 `InternalError` -> 成功路径那条
**FAILED**; 而 3 条**负向**用例仍通过 —— 因为它们本来就期望错误。
**绿灯和红灯都要看是哪一条**, 只看总数会误判: 「3 passed / 1 failed」里的
那 3 个并非「不受影响」, 而是「本来就该红」。

**共性补充**: §1.7.1 的伪装是「返回看似合理的假值」, 本条是「**做完所有前置
工作再失败**」。共同点: 两者都让人**高估完成度**。排查此类代码时, 该看的是
「最后一步是不是真的存在」, 而不是「前面几步做得像不像」。

### 1.10 WS 广播中枢: 文档声称的「占位 broadcast channel」根本不存在 (2026-10-03 已修)

**发现的矛盾** (三处独立文档都这么说):

| 位置 | 声称 |
|---|---|
| `ws/handler.rs` 模块文档缺口 3 | 「ForceDisconnect broadcast: **占位 broadcast channel**」 |
| `ws/mod.rs` 模块文档 | 同上 |
| `ws_frames.rs` 模块文档 | 「aux-13 §1.2 **服务端 11 类**」, 实际枚举只有 9 个变体 |

但代码里**没有任何 broadcast channel**。所谓「占位物」是**文档虚构的** ——
与 §1.8.2 的 `auth_ok`(凭空发明的 wire 帧)同族, 方向相反: 这次是**文档说有、
代码没有**。

**后果不是「功能少一点」, 而是端点没有意义**: `send_message` 实装后发送方能
收到 `ack`, 但**其它客户端收不到任何东西**。一个只能对发起者自己说话的 WS
端点, 对 IM 协议没有意义。

#### 实装

1. `im-protocol` 补 `ServerFrame::MessageNew { message: WireMessage }`(aux-13 §1.2.5)
2. 新建 `im-gateway/src/ws/hub.rs`:
   - `WsHub` = 单个 `tokio::sync::broadcast` 通道(容量 1024), `main.rs` 以
     `web::Data` 注入为**进程内单例**
   - `Audience` **三态**投递范围判定 + `should_deliver` 纯函数
   - `to_wire_message`: `Message` → `WireMessage`
3. `WsSession` 加 `conversation_ids` 集合, 鉴权时**快照一次**
4. `run_ws_loop` 的 `select!` 加广播分支; `handle_send_message` 落库后 publish

#### 设计决策: 为什么选「单通道 + 连接侧过滤」

per-conversation 通道需要动态增减订阅, 而 `select!` 无法对「数量不定的一组
receiver」做分支(除非 `FuturesUnordered`, 复杂度显著上升)。单通道 + 内存过滤
同等正确且简单得多。

#### 三处「静默出错」的地方 —— 本条的真正价值

这三处都不会编译报错、不会 panic, 只是**结果悄悄是错的**:

1. **广播过滤是安全边界, 不是优化**: 不过滤则用户 A 收到用户 B 所在会话的
   消息 = **跨会话数据泄漏**。所以 `Audience::All` 分支**自己判鉴权状态**
   (未鉴权连接即使「看起来该收」也不能收, 否则成为「连上就能听」的旁听入口)。
2. **`Option<ConversationId>` 二态签名是陷阱**: 初版 `frame_conversation` 用
   `None` 表示「与具体会话无关, 所有人都该收到」, 把两类语义完全不同的帧混成
   同一个值 —— `PresenceUpdate`(确实该全局发)与 `MessageEdited` /
   `ReactionAdded`(语义上属于某会话, 但 **wire 形状不带 `conversation_id`**,
   无从判断接收方是否成员)。二态下后者会被当成「发给所有人」→ 跨会话泄漏,
   **且不会有任何报错**。故改为三态 `Audience`, 第三态 `Undeliverable`
   (含 `&'static str` 原因) 明确表示「不知道该发给谁, 一律不发」。
   `Ack` / `Connected` / `Pong` 同归此类 —— 它们承载的是某个请求的 `req_id`,
   广播出去等于把别人的请求回执塞给无关客户端。
3. **成员集合不能用 `list_user_conversations`**: 它的 `cursor` 参数**被完全
   忽略**(`PgConversationRepository::list_for_user` 形参名 `_cursor`, SQL 里
   没有 OFFSET), 且 `ConversationService` 把 limit clamp 到 50。于是
   **加入超过 50 个会话的用户会静默漏收**老会话的实时消息 —— 无任何报错,
   用户只会以为对方没发言。故新增 `list_all_memberships_for_user`(只 SELECT
   id, 无 JOIN / 无 LIMIT / 无排序)。

#### 另外两处

- **`biased` 的分支顺序**: `select!` 的 `biased` 按书写顺序轮询, 所以 tick 必须
  排在广播**之前** —— 否则通道一旦有积压帧就每次命中广播分支, tick 永远轮不到,
  **广播洪水能把心跳超时判定饿死**(正是 §1.1 缺口 #H 刚修掉的那类漏洞)。
- **幂等重放不广播**: 命中 `idempotency_key` 时消息在更早的请求已落库并广播过。
  客户端超时重试(很常见)若再广播一次, 所有其它在线端都会收到**同一条消息的
  第二个副本**, 而它们根本没发起过这个请求, 无从去重。

#### 顺带修掉的两个 mock 缺陷 (同族: 验证对象与被测对象不同源)

| 位置 | 缺陷 |
|---|---|
| `im-testkit::mock_ws_frames::message_new_frame()` | 名为 message_new 却返回 `ServerFrame::MessageEdited`(当时变体不存在, 注释写「占位」)。**零测试覆盖**, 所以一直没人发现 |
| `im-testkit::mock_ws_frames::wire_message_json()` | `content` 写 `{"text":"你好"}`, **少了 `"kind"` tag**, 与 `MessageContent` 实际 serde 输出不符。唯一用到它的测试恰好只断言 `id`/`sequence`/`kind`, 没碰 `content` |

现 `message_new_frame()` 返回真变体(时间戳取固定值使 mock 可复现), 并补
`message_new_frame_matches_json_mock` —— **强类型帧序列化结果与 JSON mock 逐字段
相等**。这类漂移不会有编译错误, 也不会让其它测试变红, 只有把两种表述放在一起
比才会暴露。

#### 本条**未**覆盖的边界 (如实声明)

**没有 WS 端到端测试** —— 本仓库无任何 WS 客户端依赖(`Cargo.lock` 里只有
`actix-http 3.13.3`, 无 `tungstenite` / `awc` / `actix-test`), 加依赖需 crates.io
而 TLS 被本地代理掐断。故已覆盖的是: 投递范围判定(10 个变体逐一断言)、
过滤规则(含未鉴权连接、非成员、不可投递帧)、真实 broadcast 通道行为
(订阅/无人订阅/Lagged)、`Message → WireMessage` 转换(含坏 content 报错)。
**未覆盖**的是「帧真的写到了 socket 上」以及「两条真实连接之间的端到端投递」。

#### 变异测试 —— 证明这些断言有判别力

绿灯只证明「没报错」。故对 `should_deliver` 故意注入两处缺陷并实测:

| 注入的缺陷 | 变红的测试 | 失败信息 |
|---|---|---|
| `Audience::Conversation(_) => true`(摘掉成员校验) | `typing_and_recall_target_their_conversation` | `!should_deliver(&Audience::of(&typing), &authed_session(&[other]))` — **非成员收到了他人会话的帧** |
| 同上 | `unauthenticated_connection_receives_nothing` | 未鉴权连接也收到了会话帧 |
| `Audience::Undeliverable(_) => true` | `frames_without_conversation_id_are_never_delivered` | 不可投递帧被投出 |

3 FAILED / 7 passed。两个变异**分别**被不同测试捕获(而不是全部倒在同一条),
证明覆盖确实是按分支对齐的; 7 个正向测试保持绿, 说明不是无差别失败。
恢复正确实现后全 workspace **325 passed / 0 failed**。

**过程中测试自己抓到一处我写错的断言**: `to_wire_message_roundtrips_content_and_state`
原写 `serde_json::to_string(&w)` 却断言 `"type":"message_new"` —— `type` tag 来自
外层 `ServerFrame` 枚举, 序列化 `WireMessage` 本身根本没有该字段。这与
`f42f8e8` 那次「测试注释过度声称」同源: 断言写得比被测对象更强时, 先暴露的
往往是断言自己的错。

### 1.11 `recall_message` + 沿途挖出的两个「看起来存在、实则空转」的 service (2026-10-03 已修)

WS 6 类业务帧的第 3 类。实装过程中撞上两个**更早的**问题: 撤回所依赖的两处
基础设施本身是空壳。若不先修, 撤回就会「看起来可配、实则写死」。

#### A. `SettingsService` 从不读库 (危险度最高)

| 方法 | 注释声称 | 实际 |
|---|---|---|
| `load_initial()` | 「实际: SELECT id, settings FROM environments」 | 函数体是 `Ok(())` —— 从不读库 |
| `get(env)` | 「获取 env 配置」 | 从一个**永远为空**的 `HashMap` 取值, 任何 env 都落 `unwrap_or_default()` → **恒返回 120s** |
| `invalidate(env)` | 「收到 Valkey pub/sub 失效广播后重载」 | 只从一个空 map 里删一个不存在的键, 从不重读 |

与 §1.10 那个「占位 broadcast channel」同族(文档说有、代码没有)。**但危害更大**:
`SettingsService` 存在、类型正确、方法名规范, 代码读起来完全符合 aux-04 §B.4
不变量「撤回时间窗由 `environments.settings...` 控制, **不能写死**」——
若当初把撤回时间窗接到 `get()` 上, review **看不出任何问题**, 而线上每个
环境的时间窗都是 120s。这比明写死更难发现, 因为它骗过了检查。

**修法**: 改为真读 `environments.settings` JSONB(`SELECT settings FROM
environments WHERE id = $1`)。三个刻意的设计:

- **不缓存**。原设计有 `HashMap` + Valkey 失效广播, 但失效通道属 **WBS D-4**,
  尚未落地 —— 没有失效的缓存是正确性隐患(运营改了配置, 进程内仍按旧值判定,
  且**没有任何办法**让它刷新)。MVP 每次读一次(主键索引查询, 撤回是低频操作)。
  真正的缓存随 D-4 落地时再加, 届时失效通道与缓存同时到位。
- **环境不存在返 `NotFound`, 不返默认**。「环境不存在」与「环境存在但没配该
  字段」是两件事: 前者若静默落 120, 线上表现为「撤回窗口莫名其妙变成 2 分钟」,
  极难排查; 后者才由 serde 逐字段 default 正常兜底。
- **反序列化失败不回落默认**。库里存着解析不了的 JSON 时静默用默认值, 会让
  运营以为自己的配置生效了(实际没生效)。

**没有新增 `EnvironmentNotFound` 错误码**: 新增 `AppError` 变体必然新增一个
wire 错误码, 而 aux-03 §B 是唯一权威表、新增码属协议变更,
ImplementationSpec 处于 `[PROTOCOL-FROZEN]`。已注册的 `NOT_FOUND` 语义覆盖。

#### B. `EventPublisher::publish` 写死单一事件类型

签名原为 `publish(&self, topic: &str, payload: &MessageCreatedEvent)`。这不只是
风格问题 —— 它让**除 created 之外的任何事件都发不出去**: aux-04 §B.4 不变量
要求「转换必须 publish 事件 `im.message.{recalled,deleted}` 供其他 pod 同步」,
但加 `MessageRecalledEvent` 时编译器直接拒绝(该不变量在**类型层面**就无法满足)。

`DetailedDesign.md` §publisher.rs 本来写的就是 `publish(&self, topic, payload: &[u8])`
—— 即**规范是对的, 实现偏离了规范**。故此项是实现回归规范, 不是引入新设计。

#### C. `recall_message` 本身 (per aux-04 §B.4 转换表 line 241)

| from | event | to | guard | 失败码 |
|---|---|---|---|---|
| `sent` / `delivered` / `read` | `recall_message` | `recalled` | actor = sender 且 `now - created_at ≤ recall_window` | `RECALL_WINDOW_EXPIRED` / `FORBIDDEN` |
| `recalled` | 任何 | 拒绝(终态) | — | `INVALID_STATE_TRANSITION` |
| `deleted` | 任何 | 拒绝(终态) | — | `INVALID_STATE_TRANSITION` |

两处刻意的不确定性处理:

- **边界用 `>` 而非 `>=`**: 转换表写的是 `≤` 才允许, 所以**恰好等于**窗口长度
  时**仍应允许**。差一个字符就是差一个语义, 已用
  `recall_window_boundary_is_inclusive_at_exactly_the_limit` 锁住
  (aux-04 §F 要求「边界 ±1s」测试)。
- **`read → recalled` 允许**: aux-04 的**转换图** line 225 在这条边上标了
  「V1+ 评估是否允许」(GAP-3), 而**转换表** line 241 明确把 `read` 列入允许
  来源集。此处以转换表为准(它才是规范性那张表), 且 GAP-3 的原文问题在**展示**
  层面(「已读撤回是否还显示"已读"标识?」), 不是转移本身是否允许。
  若 PM 认为应禁止, 只改 service 的状态守卫即可, 落库与事件逻辑不受影响。

**`recall_window` 由调用方传入而非 service 自读**: 读 settings 需要
`EnvironmentId`, 而 `MessageService` 从 `messages` 行拿不到它(要经
conversation), 构造参数里也没有 settings 依赖。故与 `edit_message(max_size_bytes)`
同一约定: 参数传入, 由 gateway 用 `SettingsService` 解析后传进来。service 因此
可脱离 DB 单测, 且「值从哪来」只有一处。

**广播**: `MessageRecalled` 带 `conversation_id`(aux-13 §1.2.7), 因此是
**当前唯一可安全广播的变更类帧** —— 广播中枢能判断接收方是否成员。
(`MessageEdited` / `ReactionAdded` 不带, 判为 `Undeliverable`, 见 §1.10。)

#### 测试

新增 7 个撤回真 PG 用例 + 5 个 `SettingsService` 真 PG 用例, 覆盖: 时间窗边界
两侧(120s 恰好允许 / 121s 过期)、非 sender `Forbidden`、两个终态
`InvalidStateTransition` 且**状态未被改动**、`read → recalled` 允许、事件确实
发出且 `message_id` 是被撤回的那条; `SettingsService` 读真库(写入 777 而非
默认 120)/ 两个 env 窗口不同 / 空 settings 落 serde 默认 / 部分覆盖不丢其余
字段 / 不存在的 env 返 `NotFound`。

**测试技巧**: 时间窗用例用 `UPDATE messages SET created_at = now() - interval`
把时间**回拨**到目标偏移, 而不是 `sleep` —— 睡 120s 既慢又 flaky, 且让
「窗口 120s」与「窗口 1s」能用同一段代码测。

#### 边界测试**无法**通过挂钟观测 —— 已改为纯函数测试

最初写了一条 `recall_window_boundary_is_inclusive_at_exactly_the_limit`:
把 `created_at` 回拨到**恰好**等于窗口上限(120s), 断言仍允许。它**稳定失败**,
返回 `RecallWindowExpired`。

原因不是实现错, 是**测试前提不可达**: UPDATE 与 service 调用之间已经过去
了几毫秒, `elapsed` 实际是 120.00Xs 而非 120s。挂钟回拨只能稳定测「明显在
窗内」和「明显超窗」, 中间那一毫秒**测不到**。

修法: 把判定抽成纯函数 `within_recall_window(now, created_at, window)`, 用
**精确构造的入参**测边界(`t + 120s` vs `t`)。这比挂钟回拨**更强** ——
它精确验证了 `≤` 这一个字符, 且不依赖任何时序运气; 集成测试则退守到
「60s < 120s」这种有余量的情形, 只验接线。

**共性**: 这与 §1.9 那条「给容器加了清扫后, 断言垃圾会堆积的测试必然失败」
同族 —— **测试描述的状态在真实世界不可达**。区别在于处置: 上次是「测试前提
错、改测试」, 这次是「前提不可达、改设计让边界可测」。判别式:
**写完断言后问「这个状态在真实执行中真的可达吗? 还是会像上面那样, 在两次
操作之间就漂走了?」**

### 1.12 `mark_read` 实装 + 规范之间的一处不自洽 (2026-10-03 已修)

WS 6 类业务帧的第 4 类。`mark_read` 本身很简单, 但实现时撞上一个值得记的
问题: **三份规范对同一条 SQL 给的目标互相打架**。

#### 不自洽点

| 出处 | 说法 |
|---|---|
| `aux-07` §H.4 + `aux-08` §幂等性 | `SET last_read_sequence = GREATEST(last_read_sequence, $1)` —— 保证单调 |
| `aux-04` §C.4「mark_read 高频」行 | 「`last_read_sequence` 仅在 `new_sequence > last_read_sequence` 时 UPDATE, **减少 80% 写**」 |

照 `GREATEST` 写能达成前者, **但达不到后者**: PG 的 UPDATE 即使把列写成
**完全相同的值**, 仍会产生新的行版本(tuple version)—— 这是 MVCC 的代价。
而「客户端重复上报同一个 sequence」是极常见的常态(每次收到新消息都上报当前
最大 seq, 而多批消息的 seq 可能重复上报), 所以 `GREATEST` 在最需要省写的
场景里**一次都省不下**。

**修法**: 把守卫写进 `WHERE` 而非 `SET`:

```sql
UPDATE conversation_members SET last_read_sequence = $1
WHERE conversation_id = $2 AND user_id = $3 AND last_read_sequence < $1
```

单调性与省写由同一个 `WHERE` 同时保证, 不需要 `GREATEST`。

**并把它变成可观测的测试**: 用 PG 的 `xmin`(该行最近一次 UPDATE 所在事务 id)
断言「未推进时 `xmin` 不变」。若实现退化成 `GREATEST`, 值虽然没变但**行版本会
变**, `xmin` 随之改变 —— 那正是「没省下写」的直接证据。
(`repeated_mark_read_skips_the_write`)

#### 其它三处判断

- **非成员必须 `Forbidden`**: 转换表 guard 写「receiver 在线且为会话成员」。
  若不校验, 任意用户能给任意会话写 `last_read_sequence`, 等于让他影响别人的
  未读数。`advance_last_read_sequence` 的 `Ok(false)` **无法区分**「未推进」
  与「不是成员」(两者都是 0 行受影响), 故 service 先查成员关系再更新。
- **ack 不带 `data`**: aux-13 §1.2.2 的 `AckData` 字段是 `message_id` +
  `sequence`, 语义都指向**消息**。已读回执没有 message, 把 `last_read_sequence`
  塞进 `sequence` 会让同一字段在两类响应里含义不同。故 `data: None`
  (`skip_serializing_if` 使该字段整个不出现)。
- **`Ok(false)` 仍回 `ok=true`**: aux-08 明确把 mark_read 列为「幂等 UPDATE」,
  重复上报是**成功**而非错误。

#### fanout 缺口: `ServerFrame` 没有已读回执变体

转换表 line 239 的 effect 写「UPDATE last_read_sequence **+ fanout**」, 但
`ServerFrame` 的 10 个变体里**没有 read receipt 帧**。往任何现有帧上捎带都
是凭空发明 wire 形状 —— 与 §1.6 的 `UNSUPPORTED_OPERATION`、§1.8.2 的
`auth_ok` 同一类错误。故本条只做可确证的 UPDATE 部分, fanout 记为待规范
所有者补帧。`ConversationService::mark_read` 返回 `bool`(`advanced`)正是为了
让将来补上 fanout 时能判断「值没变就别广播」, 不必再改接口。

#### 测试

6 个真 PG 用例: 首次推进 / 更小的 sequence **不回退** / 重复上报幂等 no-op /
重复上报**不产生新行版本**(`xmin` 断言) / 非成员 `Forbidden` **且不改动他人
读指针也不凭空建行** / A 读不影响 B 的读指针。








---

### 1.13 `react` + `typing` 实装, 6 类业务帧收官; 以及一处需要规范所有者决断的 wire 缺口 (2026-10-03)

至此 aux-13 §1.1 的 6 类业务帧**全部落地**。但收官时暴露出一个模式, 值得
单独记: **有三类帧的「持久化部分」能做、「广播部分」做不了, 原因完全相同**。

#### `ReactionService`: 权限边界是它存在的全部理由

`ReactionRepository` 只做 PK 幂等插入, **不校验任何东西**。所以「谁能对哪些
消息加表情」这条规则**完全由 service 承担**。若这层缺失, 任何持有任意
`message_id` 的用户都能对**任何**会话里的任何消息加表情。

三条刻意的设计:

- **校验必须先于写入, 不是写后回滚**。测试直接断言「越权 add 之后
  `message_reactions` 里一行都不能多」—— 若实现先插入再报错回滚, 这条会红。
- **只读列表也校验**。否则非成员能凭 `message_id` 枚举「谁对这条消息点了什么
  表情」—— 那是**会话内的用户行为信息**, 属于泄漏。「能读」不等于「无风险」。
- **emoji 校验排在两次库查询之前**。空/超长 emoji 不该为一个显然无效的输入
  去查两次库。

返回 `(Reaction, bool)` 的 `bool` = `inserted`(是否真插了行)。`add` 自身幂等
但**不告诉调用方**是自己插的还是回查到的, 而广播需要这个区分 —— 漏掉它就是
「重复 reaction 被广播多次」的 bug。代价是多一次 `list_for_message` 查询;
reaction 是低频操作, 划算。

#### `typing`: 不查库, 且这是刻意的

typing 是全协议**最高频**的帧(打字时可能每秒一次), 而 `aux-13` §1.1.7 未给它
定义任何 guard。**不加**成员校验, 理由: 每帧一次 DB 往返在热路径上不成比例,
而**安全性并不依赖它** —— 广播中枢的 `Audience::Conversation` 过滤保证非成员的
typing 帧投不出去。代价是非成员会拿到 `ok: true` 却看不到效果, 可接受:
typing 是瞬时信号, 客户端没有任何可观测结果依赖它。

> 判别式: **这个守卫是为了挡泄漏, 还是为了让客户端拿到准确反馈?**
> 前者必须放在**投递前**; 后者可以省。

#### wire 缺口: 三类帧「能存不能广播」

| 帧 | 缺什么 | 后果 |
|---|---|---|
| `MessageEdited` (aux-13 §1.2.6) | 不带 `conversation_id` | 编辑无法实时同步 |
| `ReactionAdded` (aux-13 §1.2.8) | 不带 `conversation_id` | reaction 无法实时同步; 且广播它还会顺带泄漏「谁对哪条消息点了什么表情」 |
| (无已读回执变体) | `ServerFrame` 根本没有这个帧 | 已读无法实时回执给发送方 |

前两个是**同一个根因**: 帧不带会话标识, 广播中枢无法判断接收方是不是成员,
发给所有人即跨会话泄漏。第三个更彻底 —— 连变体都不存在。

**三者都属协议变更**(`[PROTOCOL-FROZEN]`), 不由实现方拍板。实现侧的选择是:
持久化部分照做(那是可确证的), 广播部分**不发明**。`Audience::Undeliverable`
就是为这种情况准备的第三态(见 §1.10)。

**这已连续三次撞到「wire 形状挡住了正确实现」**(`UNSUPPORTED_OPERATION`、
`auth_ok`、以及这次的三个帧)。规律是: 规范给的是**样例 JSON** 而非**结构定义**
—— 样例里没写的字段, 实现方就无权补。若规范改为给**字段表 + 可空性**,
这类缺口会从根上消失。

#### 测试

7 个真 PG 用例: 成员可加且落库 / 重复 add 报 `replay` 且库里仍一行 /
非成员加表情 `Forbidden` **且不留任何行** / 非成员列 reaction `Forbidden` /
不存在的消息 `MessageNotFound` / 空 emoji 与超长 emoji `Validation`。

### 1.14 4 个消息动作端点此前**只有 WS 帧, REST 侧完全没有** (2026-10-03 已修)

`edit` / `recall` / `react` / `mark_read` 这四项能力在 2026-10-03 之前**只有
WebSocket 帧**能触达, REST 侧一个端点都没有。这对「商业产品标准」是硬伤:
服务端 SDK、后台任务、非 WS 客户端(Web / 桌面离线补传)全都够不着 —— 而这
四项能力**已经**在 service 层实装完毕。

也就是说, 缺的不是「能力」而是「**出口**」: 一个能用的函数, 没有任何非 WS
路径能调用它。

#### 顺带发现: DetailedDesign **自相矛盾**

`DetailedDesign.md` 的端点→需求映射表逐条写着:

```
| §5 PATCH  /v1/conversations/{id}/messages/{msg_id}          | 编辑消息   | ... | §7 |
| §5 POST   /v1/conversations/{id}/messages/{msg_id}/recall   | 撤回       | ... | §7 |
| §5 POST   /v1/conversations/{id}/messages/{msg_id}/reactions| reaction   | ... | §7 |
| §5 POST   /v1/conversations/{id}/read                       | 已读回执   | ... | §7 |
```

—— 明确标注这四个端点属于「**§5**」。但 DetailedDesign §5 的标题是
「REST API **完整**清单(MVP)」, 其表格里**根本没有**这四个端点。

更广地说, 两份规范对「端点全集」的认知**不同**:

| 规范 | 有 | 无 |
|---|---|---|
| `BasicDesign §7` | recall / reactions / read / PATCH edit | friends / media / me |
| `DetailedDesign §5` | friends(4) / media(2) / me(2) | recall / reactions / read / PATCH edit |

**本 commit 只做两份规范「都指向或其一明确列出」的消息类 4 个** —— 它们的
意图无歧义(映射表点名 + BasicDesign §7 列出 + service 已就绪)。friends /
media / me 那 8 个端点**不在范围**: 它们的 service 层大多尚不存在
(relationship 有仓储但无 service; `/me` 需要一个尚未定义的用户资料读写面),
硬做只能凭空发明 wire 形状 —— 那正是本项目记了三次的错误形态(§1.6 /
§1.8.2 / §1.13)。

#### 两条路径共用同一条校验链

四个 handler 全部只做「解析 → 调 service → 映射状态码」, **不重写任何业务
规则**。仅原 sender / 终态拒绝 / 时间窗 / 成员校验 / 单调不回退全在 service
里。若 REST 另写一遍, 两条路径迟早漂移, 而漂移的那一侧就是越权漏洞。

#### 三处刻意的设计

- **路径里的 conversation_id 必须与消息实际归属一致**。service 的编辑/撤回
  只校验「是否原 sender」, **不**校验路径里的会话; 不在 handler 层对齐的话,
  客户端会拿到「在会话 A 编辑成功」的响应而资源其实在 B —— URL 说谎, 且让
  按 conversation 做的审计与限流全部错位。不一致返 **404** 而非 403: 后者会
  顺带确认「该 id 对应的消息存在于别处」。
- **响应码不自造**: 无规范规定, 故失败一律走 `json_response(code, ..)`,
  状态码由 `ErrorCode::http_status()` 这个**单一真源**决定(与既有 messages
  handler 完全一致)。成功侧用 REST 惯例且与实际结果对应:
  `PATCH` 200 / `recall` 200 / `reactions` **201**(新插入)或 **200**(幂等
  重放)/ `read` 200。
- **`/read` 回服务端当前读指针, 不回显请求值**。请求一个更小的 sequence 是
  **成功但无效果**(aux-08 幂等约定); 回显请求值会让客户端以为读指针退了。

#### 顺带修掉的两个小缺陷

| 位置 | 缺陷 |
|---|---|
| `messages.rs` `MessageResponse::from` | `state` 字段用 `serde_json::to_value(..).ok().and_then(as_str).unwrap_or_else(\|\| "sent")` —— 序列化一旦失败, 一条**已撤回**消息会以 `"state":"sent"` 返回, 客户端于是认为它仍可编辑。改用 `MessageState::as_str()`(单一真源, **无失败分支**) |
| `conversations::get` 内部错误 | 散写的 `.map_err(\|e\| { ... })` 漏了 `tracing::error!`, 500 会变成完全不可诊断的 500(连服务端日志里都没有原因)。抽 `internal_error()` 统一出口 |

#### `GET /v1/conversations/{id}` 从 placeholder 摘下来

此前该路由指向 `placeholder::conv_get`(恒 501), 注释写「out-of-scope」——
但 `ConversationService::get` 早已实装, 所以这不是「做不了」而是「没人接线」。
一个**能做的端点**挂着 501, 会让 SDK 按 501 决定降级策略, 比直接 404 更糟。

**成员校验必须在这一层做**: `get()` 只按 id 查, 不管调用方是不是成员; 不校验
的话, 任何持有 access token 的用户都能凭 id 枚举任意会话的 `metadata`
(群名、公告等)。非成员返 404(与 `GET /v1/conversations` 把非成员会话排除在
列表外的语义一致, 且不泄漏「该 id 是否存在」)。

### 1.15 上一条 commit 的 4 个端点**状态码是我选的, 却一个端到端测试都没有** (2026-10-03 已修)

§1.14 把 4 个端点接上线时, 它们的**状态码全部无规范可依** —— `201`(新插入) vs
`200`(幂等重放)、`404` vs `403`、`409` 全是我按 REST 惯例定的。既然是自定的,
就不该只靠 service 层的真 PG 测试兜底: service 层根本走不到
`init_service` → 路由匹配 → `Json` extractor → `json_response` 的状态码映射这条链。

#### 覆盖了什么(12 个)

| 组 | 用例 |
|---|---|
| 路由 + 状态码 | `patch_edit_returns_200_with_updated_content` / `reactions_returns_201_then_200_on_replay` / `recall_returns_200_with_state_recalled` / `recall_twice_is_invalid_state_transition` |
| 权限 | `edit_by_member_who_is_not_the_sender_is_forbidden` / `reactions_by_non_member_is_forbidden` / `mark_read_by_non_member_is_forbidden` / `all_action_endpoints_require_authentication` |
| 路径一致性 | `message_in_a_different_conversation_returns_404` |
| 已读语义 | `mark_read_advances_and_reports_server_value` / `mark_read_rejects_negative_sequence_with_400` |
| 读侧泄漏 | `conversation_get_hides_metadata_from_non_members` |

#### 变异测试: 4 处注入, 4 处被捕获, 各由**唯一**一个测试抓住

绿灯只证明「没报错」, 不证明「测到了东西」。故逐个摘掉已实现的门禁再跑:

| 注入的缺陷 | 唯一失败的测试 |
|---|---|
| `ensure_message_in_conversation` 的归属比对短路 | `message_in_a_different_conversation_returns_404` |
| `ReactionService::add_reaction` 的成员校验去掉 | `reactions_by_non_member_is_forbidden` |
| `ConversationService::mark_read` 的成员校验去掉 | `mark_read_by_non_member_is_forbidden` |
| `conversations::get` 的成员校验去掉 | `conversation_get_hides_metadata_from_non_members` |

四处互不覆盖 —— 摘掉任意一个, 只有对应那条变红。这同时证明 e2e 真的连上了真
PG(否则拿不到真实状态码), 而不是被 `let Some(p) = .. else { return }` 跳过。

#### 但**这个跳过本身仍是一个假绿灯向量**, 记在这里不藏

13 个 e2e 沿用 `auth_handlers` 的约定: 连不上 `DATABASE_URL` 就 `return` 而**不是**
`fail`。好处是 CI 无 PG 时不阻塞, 代价是**一旦连接失败, 13 个用例全部静默通过**,
且没有任何标记能区分「跑过了」与「没跑」。本地这 13 个是跑过的(上面四次变异
就是证据), 但 CI 侧这一条**目前无法保证**, 需要 D-4 之后接 PG service container
才能消掉。替代方案(连接失败即 fail)会让无 PG 的开发者本地全红, 未与使用者确认
前不擅自改。

#### 一个**故意没写**的用例

原打算写 `edit_by_non_member_is_forbidden`。写完发现
`MessageService::edit_message` **没有**独立的成员校验 —— 非成员同样被 sender
条件挡住。写出来会是「同一个 403、同一个原因」的重复断言, 且测试名会让人误以为
存在成员门禁。故删掉, 改在 sender 那条用例的注释里点明「这里只有 sender 一道
门禁」。夹具相应拆出 Bob(成员但非 sender)与 Carol(非成员)两个人, 否则「非
sender」这个用例实际测的仍是「非成员」。

---

### 1.17 `deploy/k3s/dev/` 清单**从未被执行过** —— 6/8 环境变量名对不上, 且**没有任何东西能构建镜像** (2026-10-03 已修配置层, 部分修清单)

F-2 (K3s 部署) 一直标着「受阻于 F-1 Docker daemon 间歇故障」。2026-10-03
Docker daemon 恢复后逐项核对, 发现阻塞**不止 Docker**:

#### 根因 1: 清单的环境变量与 `AppConfig` 的字段名**大面积不一致**

`AppConfig::load()`(即 `main.rs` 调用的入口)传 `config_dir = None`, 所以
**生产环境不读任何 TOML 文件** —— 容器里唯一的配置来源就是环境变量。而映射
规则是 `IM_<UPPER_SNAKE>` → `AppConfig` 同名小写字段。逐项对比:

| 清单里写的 | 代码里的字段 | 结果 |
|---|---|---|
| `IM_HTTP_PORT` | `http_port` | ✅ |
| `IM_JWT_SIGNING_KEYS` | `jwt_signing_keys` | ✅(名字对, 但**值**配不出来, 见根因 2) |
| `IM_DATABASE_URL` | **`IM_POSTGRES_URL`** | ❌ 名字错 |
| `IM_REFRESH_TOKEN_PEPPER` | **`IM_REFRESH_PEPPER`** | ❌ 名字错 |
| `IM_NATS_URL` | **`IM_EVENT_PUBLISHER`**(嵌套 struct) | ❌ 无此字段 |
| `IM_VALKEY_URL` | (无 — D-4 未落地) | ❌ 无此字段 |
| `IM_ENV` | (无) | ❌ 无此字段 |
| `IM_GRPC_PORT` | (无 — **im-gateway 没有 gRPC server**) | ❌ 无此字段 |

`postgres_url` 是**必填**字段(无 `#[serde(default)]`), 名字写错即反序列化失败
→ 进程起不来。所以这份清单不是「还没跑」, 是**跑必然红**。

#### 根因 2: 结构化字段**结构上**无法从环境变量配置

即使把名字改对, 仍然起不来。实测(新增 `crates/im-common/tests/config_env_only.rs`
的探针, 已转正为正式测试):

```
config load failed: invalid type: found string "v1:somekey",
expected a sequence for key "default.jwt_signing_keys"
```

`env_value_to_toml` 原本只处理标量(bool / 整数 / 浮点 / 字符串), 于是
`jwt_signing_keys: Vec<SigningKeyConfig>` 拿到扁平字符串就报错。而
`event_publisher`(嵌套 struct)与 `server_secrets`(`HashMap`)同样配不出来 ——
后者一坏, S2S token exchange 的 HMAC 验签在容器里就不可用。

**既有测试为什么没抓到**: `config.rs` 的测试只覆盖了「必填字段**缺失** → Err」。
也就是说「四个必填变量都给了, 能不能真的加载出来」**从未被验证过**。

**修法**: `[` / `{` 开头的值按 JSON 解析后转成 TOML 字面量; JSON 解析失败则
原样透传给 TOML 解析器(于是直接写 TOML 内联表也仍可用, 且语法错误照样报错)。
这同时**取代**了 docstring 里声称但从未实现的 `IM__SECTION__KEY` 嵌套语法。

> 一个自己踩的坑: 我第一版实现是「JSON 原样透传」, 并在注释里断言
> 「TOML 的数组/内联表与 JSON 语法高度重合」。**这是错的** —— JSON 用
> `"k": v`, TOML 内联表用 `k = v`。测试当场报 `TOML parse error`。
> 「我以为两种格式兼容」和「我验证过它们兼容」之间隔着一个测试。

#### 根因 3: 仓库里**没有 Dockerfile**, 也没有任何 CI build job

`im-gateway.yaml` 引用 `ghcr.io/yourorg/im1.0-im-gateway:latest`, 但没有任何
东西能构建它。`migrate-job.yaml` 引用的 `im1.0-im-migrate` 镜像同理(仓内
**没有** migrate 二进制, 只有 `sqlx migrate run` 这个外部命令)。

**新增 `Dockerfile`**(多阶段 / 显式钉 `rust:1.98.1` / `--locked` / 非 root)与
`.dockerignore`。缺 `.dockerignore` 的后果很具体: 本机 `target/`(Windows
编译产物, 数 GB)会被整个塞进构建上下文。

#### 顺带删掉一处**不存在的端口**

`im-gateway.yaml` 的容器与 Service 都声明了 `grpc 9000`, 但
`grep -rn 9000 crates/im-gateway/src` **无命中** —— im-gateway 没有 gRPC
server(proto 里的 `CoreService` 由 gateway **进程内**直接调 im-core, 不走
网络)。声明一个永不监听的端口, 只会让 Service 的使用者以为有可连的端点。
已删除并注明原因。

#### 仍未闭合

- **`Dockerfile` 未经实际构建验证**。实测结果: BuildKit 成功 `load build
  definition from Dockerfile ... DONE`(即语法有效), 随后在拉
  `rust:1.98.1-slim-bookworm` 时报 `registry-1.docker.io ... EOF`。
  本机本地代理掐断外网 registry(与 GitHub 同一个问题), 且无 `rust:*` 缓存
  可用。故**只声称 Dockerfile 语法有效, 不声称镜像可用**。代理恢复后
  `docker build -t im1.0-im-gateway:local .` 即可验证。
- `migrate-job.yaml` 仍引用**无法构建**的 `im1.0-im-migrate`。需要一个 migrate
  目标(要么加一个 bin, 要么改用 `sqlx/sqlx-cli` 基础镜像 + 挂载
  `migrations/`)。本次未做 —— 它不影响 gateway 起不来这个已修的问题。
- `readyz` 当前**无条件返回 200**, 不检查 PG/NATS/Valkey 可达性
  (`DetailedDesign §5` 要求「PG/Valkey/NATS 全部可达才 200」)。即 k8s 会把
  一个连不上数据库的实例判为 ready 并把流量打过去。**未擅自改**: 该改哪些
  依赖算「可达」涉及部署形态决策, 且 Valkey(D-4) 根本不存在。
- F-2/F-3 的端到端仍**未跑通**: 本机 Docker Desktop 的 k8s API 超时
  (`172.28.176.169:6443 context deadline exceeded`), 而项目目标是 K3s。

### 1.18 `/readyz` 无条件返回 200 —— k8s 会把流量送给一个每个端点都 500 的实例 (2026-10-03 已修)

§1.17 记下「readyz 恒返 200」时写的是「未擅自改, 因为哪些依赖算可达涉及部署
形态决策」。**复核后这个理由不成立**: `DetailedDesign §5` 已经点名了要求 ——
「就绪检查(PG/Valkey/NATS 全部可达才 200)」。那是规范, 不是待定的设计选择。
上一轮把规范已经回答过的问题当成「需要决策」, 是拖延的一种形式。

#### 查什么, 不查什么

| 依赖 | 查不查 | 理由 |
|---|---|---|
| **PostgreSQL** | ✅ | 唯一有**真实失败模式**的依赖: 每个业务端点都要它。查不到 = 接了流量也全部 500, 正是 readiness 该拦下的 |
| NATS | ❌ | `NatsEventPublisher` 是 stub(`_client: None`), **没有任何连接可查**。查一个恒「可达」的空壳只会给虚假安全感 |
| Valkey | ❌ | **D-4 未落地**, 配置无字段、代码无客户端。若强行查, readyz 永远 503, 整个部署起不来。宁可少查并**显式声明**, 也不要把 Pod 永久判死 |

响应体里显式写出「没查」的两项(`not_checked_stub_publisher` /
`not_implemented_D4`), 而不是让它们**不出现** —— 少一个字段, 读 `/readyz`
的人会把「没提到」误读成「查过了且通过」。

#### liveness 与 readiness 刻意**不**合并

`/healthz` 绝不碰任何外部依赖。合并的后果: 数据库一抖, 全部副本同时被
liveness 判死并重启 —— 把「一个依赖不可用」放大成「整个服务不可用且重启中」。

#### 一个容易踩的坑: 探测必须有硬上界

k8s 探针 `timeoutSeconds` 默认 **1s**。若 `SELECT 1` 挂住到 5s, 表现是探针
持续超时 → 连环失败 → Pod 反复重启 —— 一个本该只影响流量的依赖故障被放大成
CrashLoop。故 `PG_PING_TIMEOUT = 500ms`, 且由 `tokio::time::timeout` 强制,
**不依赖 sqlx 自身的连接超时**(后者可能因池里有坏连接而拖得更久)。
`pg_ping_timeout_is_well_under_the_default_probe_timeout` 锁住这个数值 ——
它是纯逻辑断言, 因为本地没有 k8s 在跑, 线上才会暴露。

#### 变异验证

把 `if ok` 改成 `if true || ok`(等价于恢复成原来的无条件 200):
`readyz_is_503_when_postgres_is_unreachable` 立刻变红, 报 `left: 200,
right: 503`。

#### 测试里一处**自己写错的前提**

最初用 `PgPoolOptions::connect()` 构造「不可达的池」, 但它对不可达端口**当场
返回 `Err(PoolTimedOut)`**, 根本走不到 handler。改用 `connect_lazy` —— 这也
更贴近生产: 池在启动时建好(那时 PG 可可达, 否则进程起不来), readiness 要
捕捉的是**之后**的不可用(PG 重启 / 网络分区 / 连接被中间件掐断)。

---

---

### 1.19 事件总线是**静默 no-op** —— 每条领域事件都被丢弃, 而系统看起来完全正常 (2026-10-03 已改为可见)

做了一次全仓空转普查(搜 `todo!` / `unimplemented!` / `占位` / `placeholder` /
`not yet implemented`), 大部分命中都已入台账且属有意保留。**唯一一条不在册的**
是这个。

#### 问题

`NatsEventPublisher::publish()` 的完整实现是:

```rust
tracing::debug!(topic, bytes = payload.len(), "NatsEventPublisher.publish (stub)");
Ok(())
```

`MessageService` 在 `send_message` / `recall_message` 之后都调它, 于是
`im.message.created` / `im.message.recalled` / `im.message.deleted`
**全部被丢弃**。而 aux-04 §B.4 转换表把「publish 事件供其他 pod 同步」
写成**不变量** —— 这条不变量从未被满足。

**为什么说它比报错更糟**: 三个渠道都指向「正常」——
1. 返回值是 `Ok(())`, 调用方的错误处理分支永不进入;
2. 日志级别是 `debug`, **默认不可见**;
3. `/metrics` 里没有任何相关指标。

于是「跨 pod 事件同步根本没工作」这件事, 在整个运行期**没有任何外部表征**。
读代码的人(`self.events.publish(topic, &payload).await` + `Ok(())`)会以为事件
发出去了。这与 §1.11 记的 `SettingsService` 恒返 120s 是同一族问题:
**代码读起来是合规的, 实际什么都不做**。

#### 修法: 不改语义, 改**可见性**

**没有**实装真 NATS —— 拉不到 `nats:*` 镜像(代理掐断 Docker Hub), 写一份
无法测试的实现比留 stub 更糟。**也没有**改启动语义(让网关硬依赖 NATS 属于
部署决策, 不由实现方拍板)。

改的是让这个沉默**变成事实而不是意外**:

| 之前 | 现在 |
|---|---|
| `debug!` 一行 | `warn!` 每次(默认级别可见), 带 `dropped_total` |
| 无指标 | `/metrics` 暴露 `im_events_dropped_total` counter |
| `connect()` 静默 | `connect()` 发一条 `warn!`, 明确声明「事件会被丢弃」 |
| 无 | 模块文档说明「**不声称事件已发出**」 |

面板上把 `rate(im_events_dropped_total[5m])` 画出来, 应当恒为 0; D-3 接线后它
归零, 之后任何非 0 都是真丢事件。

#### 变异验证

把 `EVENTS_DROPPED.fetch_add(1, ..)` 改成不累加(即恢复原 no-op 行为):
`metrics_expose_the_dropped_event_counter` 立刻变红, 报 `left: 0, right: 1`。
该用例同时锁住两件容易漏的事: ① 计数真的在累加; ② 计数真的被挂到了
`/metrics` 上(加了计数器却忘了暴露, 是同一种静默)。

#### D-3 本身仍未实装

真 NATS 连接 + `async_nats` publish 需要一个可连的 NATS server 才能测。
依赖 Docker Hub 恢复。WBS D-3 状态不变。

---

### 1.20 仓内**没有任何代码能应用迁移** —— 7 份 SQL 从未由本仓的代码执行过 (2026-10-03 已修)

`deploy/k3s/dev/migrate-job.yaml` 引用 `im1.0-im-migrate:latest` 执行
`sqlx migrate run`, 但**没有任何东西能构建那个镜像**(无 migrate 二进制、
无 CI build job)。凑合的办法是换 `sqlx/sqlx-cli` 基础镜像并挂载
`migrations/` —— 那等于把 schema 版本与镜像版本拆开, 于是会出现
「镜像里的 SQL 比镜像里的代码旧」这种最难排查的状态。

#### 顺带暴露的一件没人提过的事

`crates/im-gateway/tests/migration_smoke_pg.rs` 是**查 schema** 的测试, 它
要求 `DATABASE_URL` 指向一个「**已应用迁移**」的库 —— 谁应用的、什么时候应用
的, 它一概不问。

也就是说: 那条测试能证明「schema 是对的」, 但**证明不了「我们的代码能把它
变成对的」**。7 份 migration 在本仓的历史上从未被仓内代码在任何环境执行过;
它们「看起来是对的」是因为有人(某次手工操作)在某个库上跑过, 之后的测试都复用
那个已建好的库。

#### 修法: `sqlx::migrate!` 编译期内嵌

新增 `crates/im-migrate`(bin + lib):

- `sqlx::migrate!("../../migrations")` 把 7 份 SQL **编译期内嵌进二进制**。
  多一份文件、或者改了 SQL 忘记重建镜像, 都会在**构建期**炸, 而不是上线后
  行为诡异地不一致。
- 走 `run()` 而非 `run_direct()`: 后者不写 `_sqlx_migrations` 表, 于是无法
  判断哪些已应用, 重复跑会撞约束。
- 环境变量用 `IM_POSTGRES_URL` 而非 sqlx CLI 惯例的 `DATABASE_URL` —— 整个仓
  的配置读取都走 `IM_<UPPER_SNAKE>`(`AppConfig`), 迁移工具没有理由成为例外。
  顺带更正: `migrate-job.yaml` 原先写的 `IM_DATABASE_URL` **同样对不上**。
- `Dockerfile` 加 `--target migrate`, **复用同一个 runtime stage** 只换
  ENTRYPOINT, 于是两个镜像的 OS / CA / 用户必然一致。

#### 新增的真实验证(本项目第一次)

`crates/im-migrate/tests/migrations_e2e.rs`: 建一个**全新的空库** → 跑全部
7 份迁移 → 断言 14 张表建成 / 0007 的两列与两条索引都在 / 历史表 7 条全 success
→ **再跑一次验证幂等** → 无条件 `DROP DATABASE` 清理。

「重跑必须幂等」这条不是凑数: 迁移 Job 的 `backoffLimit: 3` 意味着它会被反复
触发, 不幂等就会在重试里撞约束 —— 那正是「Job 永远失败」的经典原因。

#### 三处自己写错的, 都被测试/工具当场抓住

1. `with_database` 手搓 URL 解析**漏掉路径分隔符**, 拼出
   `postgres://im:im@localhost:5544probe`, 而 sqlx 报的错是
   `invalid port number` —— 错误信息与真正原因毫无关联。修法: 抽成纯函数
   `im_migrate::with_database` **并配单测**, 不靠连库才发现。
2. sqlx 0.9 对非字面量 SQL 有**编译期拒绝**, 连 `raw_sql` 也要
   `AssertSqlSafe` 显式豁免(书面承诺「我审计过」)。DDL 的库名不能是绑定参数,
   所以拼接无法避免 —— 于是紧挨着写一条白名单断言, 让「审计」是可执行的。
3. `migrator_embeds_all_seven_migrations` 锁住「7 份 + 版本连续」, 免得将来加了
   第 8 份却没人更新文档里的「7/7」表述。

#### 顺带拿到代理问题的**精确根因**

跑 `docker build` 时报错比之前信息量大得多:

    failed to fetch oauth token: Post "https://auth.docker.io/token":
    proxyconnect tcp: dial tcp 127.0.0.1:7897: connectex:
    No connection could be made because the target machine actively refused it

即: Docker 被配置为走 `127.0.0.1:7897` 的本地代理, 而**那个代理进程没在运行**
(主动拒绝 = 端口上没有监听)。这比「外网被掐断」具体得多 —— 要么把代理起起来,
要么把 Docker 的代理配置摘掉。两个 target 的 Dockerfile **语法均已验证通过**
(`load build definition ... DONE`), 但**实际构建仍未验证**。

---

### 1.21 迁移撞上「手工建的库」时报的错**指向错误的方向**, 而不是没说清 (2026-10-03 已修)

§1.20 修好了「没有代码能应用迁移」, 立刻暴露了下一个问题: 拿新的 `im-migrate`
去跑本机的 `im_test` 库, 报的是:

```text
while executing migration 1: error returned from database:
  trigger "trg_environments_before_update" for relation "environments"
  already exists at line 765
```

真实原因: **那个库的 schema 是手工建的**, 从未记进 `_sqlx_migrations`,
迁移器于是认定「什么都没应用过」, 从 0001 重放。

#### 为什么第一个炸的是触发器 —— 纯属排在最后

0001 里 `CREATE TABLE` / `CREATE INDEX` 都带 `IF NOT EXISTS`, 于是前面那些
「已存在」全被**静默吞掉**; 而 PG 的 `CREATE TRIGGER` **没有** `IF NOT EXISTS`
这种写法, 所以第一个真正报错的偏偏是它。换个库、先炸的完全可能是别的对象 ——
**报错指向谁, 取决于谁没写兜底, 与谁才是真正的问题无关。**

#### 比「没说清」更糟: sqlx 加的前缀在往错的方向指

写台账时我先断言「报错里没有 `migration` 这个词」, 测试当场把它打脸:

```
while executing migration 1: error returned from database: trigger ...
```

`migration` 确实出现了 —— 但那是 **sqlx 自己拼的前缀**, PG 那半句里一个都没有。
这个前缀的效果是让人以为「**迁移 1 写错了, 去改 SQL**」, 而 0001 一行没错。
**往错误方向指比不指更费时间**: 顺着它走的人会去反复检查、修改本来正确的
迁移文件。已据此改写本节结论, 并把断言改成对 PG 原文断言(剥掉前缀后不得
出现 `_sqlx_migrations` / `history` / `hand` 这类能反推真实原因的词)。

#### 修法: 迁移**前**判一次, 而不是让 PG 去报

新增 `im_migrate::is_clean_target`, 判定为假时非 0 退出, 错误信息直接给出
「A) 换新库 / B) 备份后手工登记迁移历史」两条路。

**判据是「业务表在不在 + 迁移历史有几行」, 不是「历史表在不在」** —— 这是
最容易写错的地方, 两种「空历史表」必须区别对待:

| 状态 | 业务表 | 历史表行数 | 判定 | 理由 |
|---|---|---|---|---|
| 全新库 | 0 | 表不存在或 0 | **放行** | 没有任何东西可丢, 跑迁移安全 |
| 受管库 | ≥14 | 7 | **放行** | 续跑是 Job 重试的常态 |
| 手工建的库 | ≥1 | 表不存在或 0 | **拦下** | schema 来自别处, 迁移历史是空的 |

中间那行(空历史表)配的是**残留**场景: 上次运行中途失败, sqlx 先建出历史表、
在迁移 1 上失败后回滚业务表, 却把空表留在原地。**空历史表本身不是问题**
(它是失败运行的正常残骸), **有业务表却没历史才是**。

若按第一版写成「历史表存在即视为受管理」, 上面第三行会被**误放行**。
变异验证: 判据换成 `history_table > 0` 后, 唯一被点亮的是
`clean_target_rejects_hand_built_schema_even_with_an_empty_history_table` ——
证明这条测试不是凑数。

#### 「拦得值不值」的对照实验

只断言「判 false」, 这个检查就成了无法证伪的仪式。所以那条用例在同一个库上
**先判、再故意跳过检查直接跑**, 断言它**确实会炸**, 且炸在触发器上。
`crates/im-migrate/tests/migrations_e2e.rs` 现有 6 个用例, 5 种库状态各自
独立临时库, 跑完无条件 `DROP DATABASE`。

真实库已复验: `cargo run -p im-migrate -- --database-url .../im_test` 现在输出
人话诊断并以 1 退出(Job 的 `restartPolicy: OnFailure` 依赖这个码), 不再吐触发器报错。

#### 顺带修的 CI 两处

1. `test-unit` / `test-integration` 两个 job 补 `IM_POSTGRES_URL`。**不补的话
   `im-migrate` 的 e2e 会静默跳过** —— 沿用 `let Some(url) = .. else { return }`
   约定, 从输出上看不出「跑过」与「没跑」的区别(同 §1.15 的假绿灯向量)。
2. 「run migrations」从 `cargo install sqlx-cli` + `sqlx migrate run` 改为
   `cargo run -p im-migrate --release -- --database-url "$IM_POSTGRES_URL"`。
   第二个理由比省几分钟编译重要: **CI 必须跑我们真正要部署的那段代码**。
   用 CLI 建 schema 的话, 部署用的那个二进制在 CI 上从未被执行过。

`--database-url` 参数刻意支持(`--k v` 与 `--k=v` 两种形态), 解析逻辑抽成纯函数
`parse_database_url` 配单测 —— 让「连的是哪个库」出现在 workflow 文件里, 读日志
不必去猜 job 级 env 的解析结果。

---

### 1.22 WS 端点只挂在 `/v1/ws/ws` —— **按规范接的客户端根本连不上** (2026-10-03 已修)

补全仓第一个 WS 端到端测试时, 第一条路径用例直接失败, 由此发现:

| 层 | 代码 | 提供的路径片段 |
|---|---|---|
| `main.rs:246` | `.service(web::scope("/v1").configure(http::configure))` | `/v1` |
| `http/mod.rs:140`(修前) | `cfg.service(web::scope("/ws").configure(ws::router::configure))` | `/ws` |
| `ws/router.rs:12` | `cfg.service(web::scope("/ws").route(...))` | `/ws` |

**actix 的 scope 是嵌套的 —— 前缀逐层相加, 同名前缀不会合并。**
实测(`/v1/ws` → **404**, `/v1/ws/ws` → **200**), 故 WS 端点实际只挂在
`/v1/ws/ws`。而 `ws/router.rs:9` 自己的文档写「注册 `/v1/ws` 端点」,
`138-dev-plan.md:242` 也写「actix-ws 0.3 端点 `/v1/ws`」—— **两者都不是**。

后果: 按规范实现的客户端 100% 连不上, 且因为**全仓没有任何测试会真的发起
一次 WS 连接**, 这件事此前无人发现。

修法: 删掉 `http/mod.rs` 那层冗余 scope, 直接 `crate::ws::router::configure(cfg)`
—— 路径的定义权归 `ws::router`(它自带 scope 且文档写明路径)。

#### 文档对 WS 路径本身有分歧(未擅自拍板)

`aux-13` 的 wscat 样例(`wss://gateway.{tenant}.example.com/ws`)与
`Observability.md §1.1.3` 写 `/ws`; `ImplementationSpec §3.2`、
`138-dev-plan.md`、`ws/router.rs` 写 `/v1/ws`。本次按**代码既有意图 + 仓内
REST 全在 `/v1` 下的惯例**取 `/v1/ws`, 并在代码注释里写明「若规范所有者定案
为 `/ws`, 改 `ws/router.rs` 一行即可」。**需规范所有者裁决。**

#### 顺带:我自己踩进了 §1.15 记的那个假绿灯, 而且是**新写的**代码

写下这批 WS 用例后第一次运行, **6 个全「通过」**。实际: Docker Desktop 已被
关掉, PG 容器随之消失, 6 个用例**全部走 `else { return }` 静默跳过** ——
而「跳过」在 test harness 里**就是「通过」**。若不是顺手查了耗时(10.01s ≈
`acquire_timeout(10s)`), 这会作为「WS e2e 已就位」被记进台账。

**这说明 §1.15 那条不是「理论上的隐患」, 而是任何人新写 PG 相关测试时都会
默认踩进去的坑。** 两处修法:

1. **「路由挂在哪」根本不依赖 PG** —— 路径用例改用真实
   `crate::http::configure` + `TestRequest`, 断言「**非 404**」而不是 101。
   不注入 `web::Data<AppState>` 时, 路由命中会在提取 `web::Data` 时返 500,
   未命中返 404 —— **500 与 404 恰好把「路由在不在」和「handler 能不能跑」
   分开**。这类「本可不依赖外部资源、却因复用了完整夹具而被绑住」的测试,
   环境一不稳就变成静默跳过。
2. **加 `IM_REQUIRE_PG` 开关**: 值为 `1` 时, 连不上 PG **直接 panic 而不是
   跳过**。本机没 PG 仍允许跳过(否则无 PG 的开发机全红), 但 CI 明确挂了
   PG service container, 此时跳过意味着 job 配错了 —— 而配置错误会以绿灯的
   形式混过去。`test-unit` / `test-integration` 两个 job 均已设上。
   已验证: 设 `IM_REQUIRE_PG=1` 且 PG 不可用时, 4 个用例由「ok」变 **FAILED**
   并给出明确原因。

#### 验证状态

**全 7 个用例已真跑通过(2026-10-03 晚, PG 恢复后)。** 五道门带
`IM_REQUIRE_PG=1` 跑: fmt / ps1lint / errcodes(21) / clippy `-D warnings` /
test 全 0, **410 passed / 0 failed**。开关生效时任何跳过都会 panic, 故
「0 失败」同时证明**零静默跳过** —— 这比 grep 日志强, 因为
`cargo test` 默认就丢弃通过测试的输出。

其中 `broadcast_reaches_other_members_but_never_a_non_member` 是 WS 侧最要紧
的一条: Bob(会话成员)收到 `message_new`, Carol(非成员)**收不到**。

#### 跑通它们的过程中, 夹具暴露了两个真实问题(都已修)

1. **夹具签的 token 缺 `dsid` → WS 鉴权一律 UNAUTHORIZED。**
   `test_support::rest_fixture` 用的是 `TokenService::issue_access_token(&u)`,
   那个入口**刻意不带 dsid**, 其文档明写「签发登录/注册类 token 时应改用
   `issue_access_token_for_session`」。真实登录路径
   `IdentityService::issue_token_pair` 是**先建 device session 再签 token**,
   所以生产 token 带 dsid —— 即**这不是产品缺陷, 是夹具在测一个生产中
   不存在的 token 形状**。已改用 `issue_access_token_for_session`。
   (`auth_handlers.rs` 里另一处 `issue_access_token` 变量名为 `legacy`、
   注释「签旧式 token」, 是**故意**测无 dsid 的旧 token, 未动。)

2. **夹具的种子消息绕过序列号分配器 → 后续任何 `send_message` 都 INTERNAL。**
   夹具此前用**手写 SQL** 插 `messages(sequence=1)`, 而
   `conversation_sequences.next_sequence` 仍停在 1 —— 夹具造出了一个生产中
   **不可能出现**的状态。之后真实 `send_message` 拿到同一个 1, 撞
   `messages(conversation_id, sequence)` 唯一约束 -> `sqlx insert` 失败 ->
   WS 侧只看到 `INTERNAL_ERROR: internal error`。已改为让种子消息也走
   `MessageService::send_message`, 顺带不再在测试里复刻一遍生产逻辑。

#### 症状在广播链路上, 原因却在夹具里 —— 所以先断言发送方 ack

第 2 条最初表现为「Bob 收不到广播」。若照字面去查 `WsHub`, 会一路查错地方。
因此该用例现在**先断言 Alice 收到了 `ok:true` 且回显 `req_id` 的 ack**, 再去
查 Bob —— 一条分不清「动作失败」与「效果没传播」的失败信息是有害的:
它把排查方向直接带偏。

#### 附:文档自身的一处回归(已修)

上一条 §1.21 插入时, 锚点选取把本节标题 `## 2. 后续新增` 一并吃掉了 ——
表格行还在、标题没了。**编辑长文档时用「下一节标题」当锚点, 必须确认
`replace_all=false` 唯一命中, 且替换文本里要把那行重新写回去。** 已补回。

#### 另一条独立的环境事实

`task_stop` **只杀任务, 不杀子进程**: 被取消的 `cargo test` 仍在后台占着
target 目录锁, 与新起的 `cargo` 互相阻塞, 表现为「Compiling 挂住不动」。
手动清掉 `cargo`/`rustc`/`link`(按 `Path` 过滤, 避免误杀其他项目的编译)
之后才恢复。

### 1.23 `cargo audit` 长期红且**没人处理** —— 一个永远红的门禁等于没有门禁 (2026-10-04 已修)

CI 的 `cargo audit` 从建仓起一直红。它和其它红门禁的区别在于: **其它门禁红会
被人看见, 而这一条红得足够久之后就成了背景噪音。** 本次修完: `cargo audit`
**exit 0**。

守则写进 `.cargo/audit.toml` 顶部(规则比这次的具体修复更重要):
**能修的一律修, 不靠豁免; 只对「无修复版本 + 有可验证的不可达理由」的项写豁免,
且必须写明重新评估的触发条件。**

#### 直接修掉三项(不在 `ignore` 列表里)

| Advisory | 包 | 处置 |
|---|---|---|
| RUSTSEC-2026-0285 | rustls 0.23.43 | TLS 1.3 跨加密层级边界错误接受握手消息 → 升 **0.23.45** |
| RUSTSEC-2025-0111 | tokio-tar 0.3.1 | PAX 头解析错误 → 文件偷渡; **无修复版本**, 改从依赖树里**消除** |
| RUSTSEC-2025-0134 | rustls-pemfile | unmaintained; 随 testcontainers 一并从树里消失 |

**rustls 那项在生产树上, 不是 dev-only**: 路径是
`tokio-rustls ← async-nats 0.50.0 ← im-core ← im-gateway`, 故必须修。

**tokio-tar 的根因不是「版本太老」, 而是一个全仓无代码引用的死依赖。**
已用 grep 实证: `testcontainers` / `bollard` 在**任何 `.rs` 文件里都没有真实
引用**, 只出现在注释与文档(`ImplementationSpec.md` 规定用它、
`migration_smoke.rs` 的注释说「实际执行由 testcontainers 版本负责」)。
它把 `bollard → tokio-tar` 整条树拉进来, 自己却从不被调用。

故处置是**删除依赖声明**而非「声明可接受」—— 依赖树从 **492 降到 439 crates**,
漏洞从图里消失。**一个从未被调用的依赖不该出现在审计报告里**, 让它留在
`ignore` 里等于把「我们不需要它」伪装成「我们接受它的漏洞」。

#### 唯一豁免项: h2 RUSTSEC-2026-0258, 且**前提被机器守住**

h2 0.3.27 **已是 0.3 线的最后一个版本**, 没有 0.3.x 修复版。修复版 0.4.16+
属于 0.4 大版本线, 而 0.3.27 是被 `actix-http 3.13.3` 锁死的 —— 拿到 0.4.x
就得升到 actix-http 4 / actix-web 5, 那是 Web 框架的**大版本迁移**,
与「打一个安全补丁」完全不是一回事(需单独排期、单独评审)。

不可达理由是**结构性的, 不是「我们觉得没事」**: h2 只在 TLS/ALPN 协商出 HTTP/2
时才被使用, 而网关 `main.rs:251` 是 `.bind(("0.0.0.0", http_port))` ——
**明文 HTTP, 不做 TLS 终止**; `deploy/k3s/dev/im-gateway.yaml` 也只有 8080
明文端口、无 443。TLS 由上游 envoy 终止, envoy 与网关之间是明文 HTTP/1.1,
**h2 的代码路径根本不会被协商到**。

#### 关键做法: 豁免的**前提**必须被机器守住, 而不是只写在注释里

上面那条「不做 TLS 终止」写在注释里, 而**注释不会因为代码变化而失效**。
有人某天给网关加上 `bind_rustls`(完全合理的需求 —— 比如让网关直接对外),
注释仍写着「不做 TLS 终止」而暴露面已经变了; 豁免会在**没人察觉**的情况下
变成一个错误的安全假设。

故新建 `crates/im-gateway/tests/audit_precondition.rs`, 把前提做成断言:
`gateway_does_not_terminate_tls_so_h2_is_unreachable` 断言 `main.rs` 里不出现
`bind_rustls` / `bind_openssl` / `bind_ssl` / `.rustls(` / `.openssl(`; 命中即
FAILED, 失败信息直接给出二选一(撤豁免并排期 actix-web 5 迁移 / 或去掉误加的
TLS 终止), 并明写「**别只是把本测试改回通过 —— 那是把安全假设藏起来**」。
另配反向锁定 `gateway_binds_plain_http_on_its_configured_port`, 在绑定方式被
大改时给出提示。

#### 验证

- `cargo audit` **exit 0**: **439 crates、0 漏洞**、1 条 allowed warning
  (`chacha20 0.10.1` yanked —— 被作者撤回过的版本, 不等于有漏洞, 故记在
  `[yanked]` 而非 `ignore`)。
- **守卫测试有判别力, 用变异验证证明**: 向 `main.rs` 注入 `bind_rustls` 标记后,
  两条守卫测试均 **FAILED**(exit 101); 验证完已还原, `git diff` 为空、无探针
  残留。**绿灯只证明「没报错」, 判别力只能靠故意注入缺陷证明。**
- 守卫测试第一版在**干净代码上就是红的**: 路径写成了相对路径
  `crates/im-gateway/src/main.rs`, 而 cargo 跑集成测试时 CWD 是**包根**
  `crates/im-gateway/`。**一个永远失败的守卫比没有守卫更糟** —— 它会让人慢慢
  习惯「这测试本来就是红的」。已改用 `CARGO_MANIFEST_DIR` 拼绝对路径。

#### 顺带修掉三处因本次改动而**说谎**的注释

删除 testcontainers 之后, 有三处注释开始描述一个不存在的执行者。这类过时比
缺注释更坏 —— **它会让人以为某件事由别人负责**:

1. `migration_smoke.rs:100`「实际执行由 testcontainers 版本负责」→ 实改为同目录
   `migration_smoke_pg.rs` + `im-migrate` e2e(覆盖并未丢失, 只是**指错了人**)。
2. 同文件 `:99`「14 张表名在 **6** 份 SQL 中」→ 实为 **7** 份
   (`migrations/0001..0007`; 同文件 `:84` 自己的断言已经是 7)。
3. `migration_smoke_pg.rs` 顶部仍写 CI「已用 `sqlx migrate run` 建库」→ §1.21
   已改为仓内 `im-migrate`; 同时该注释只说「未设 `DATABASE_URL` 则跳过」,
   漏了 `IM_REQUIRE_PG=1` 会把跳过变成**硬失败**。

**判别式**: 删依赖 / 改配置后, grep 一遍被删符号的名字 —— 注释和文档里的
「幽灵引用」不会有编译错误, 只会静静地指向空气。

### 1.24 semgrep 门禁**从未真正运行过**, 却一直报绿 (2026-10-04 已修)

CI run `37164873402`(§1.23 之后的首次全绿)里, 我按惯例去读日志而不是只看 job 结论,
看到 Static Analysis 报 success, 而 semgrep 实际崩在:

```
ValueError: invalid rule severity value: MEDIUM
  File ".../semgrep/semgrep_main.py", line 512, in <lambda>
    filtered_rules, lambda rule: rule.severity == RuleSeverity.EXPERIMENT
```

**这比 §1.23 修掉的 3 个漏洞更严重**: 那 3 个是「已知的洞」, 而这个是**门禁本身在说谎**。

#### 两层问题, 必须分开看

| 层 | 事实 | 证据 |
|---|---|---|
| **A: semgrep 自己崩了** | `p/owasp-top-ten` 里有 **11 条规则**把 `severity` 写成 `MEDIUM`, 而 semgrep 的 `RuleSeverity` 枚举只认 `INFO/WARNING/ERROR/INVENTORY/EXPERIMENT` | 拉 4 个 pack 的 JSON 逐个统计: 只有 owasp-top-ten 出现 `MEDIUM`, 其余 3 个包干净 |
| **B: action 吞掉了非零退出码** | `returntocorp/semgrep-action@v1` 在 semgrep 崩溃后**没有**让 step 失败, job 照旧 success | 日志里 semgrep 段**没有任何 `end-action` 结论行**, 而 job 结论是 success |
B 层才是要害: **即使把 A 修好, 这个门禁仍然没有牙** —— 将来任何 semgrep 故障
(规则包改坏、引擎 OOM、超时)都会继续伪装成绿灯。action 的文档只承诺
「发现问题时非零退出」, **没承诺「工具崩溃时非零退出」**。

#### 更糟的一层: 它**一次都没跑过**

`cargo audit` 长期红 → 失败后 GitHub Actions 跳过**后续所有 step** → 而
`check error codes` / `semgrep` / `ps1 encoding lint` 全都排在 audit 之后。

所以在 §1.23 之前, 这三个 step **在 CI 上从未真正执行过一次**。日志里出现的
101 处 "semgrep" 全部来自 `Set up job` 阶段预拉 `semgrep-action` 镜像, 不是扫描。
**「本仓有 semgrep 门禁」这句话此前是不成立的。**

#### 本机复现: semgrep 在原生 Windows 上跑不了

`pip install semgrep` 能装上(PIP_EXIT=0), 但 `import resource` 是 Unix-only,
必然 `ModuleNotFoundError`。最终走 WSL: `get-pip.py --user --break-system-packages`
(Ubuntu 24.04 的 PEP 668 externally-managed 会直接拒), 再 `pip install semgrep==1.36.0`
(CI 里的同版本)。

复现结果与 CI 完全一致:

| 场景 | 退出码 |
|---|---|
| 4 个包全开(不排除) | **exit=2**(崩溃) |
| 排除那 12 条 `MEDIUM` 规则 | **exit=0**, 725 条规则扫 358 个文件, **29 findings** |

#### 排除清单的条数: 11, 不是 12 —— 差的那 1 次是**嵌套**的

最初按正则统计整份 JSON, `"severity":"MEDIUM"` 命中 **12** 次, 于是各处都写了
「12 条规则」。实跑排除 11 条就能通过, 遂回头逐条核对:

| 口径 | 数量 |
|---|---|
| 顶层规则里 `severity == "MEDIUM"` | **11** |
| 整份 JSON 里 `"severity":"MEDIUM"` 出现次数 | **12** |

差的 1 次**嵌套在规则内部**(非规则级 severity), semgrep 不会拿它去求
`RuleSeverity`, 因此不参与崩溃。**排除清单按 11 条写才是对的。**

这条差值本身是个提醒: **正则计数与结构化计数不是一回事**。若当时照着 12 写
排除清单, 清单里会多一条根本不存在的 rule id(无害, 但会让下一个人花时间
去查「这条规则是哪来的」); 反过来若少写一条, 门禁就会莫名其妙地红。

#### 29 条 finding: 门禁一开就全是真问题

| 条数 | 规则 | 处置 |
|---|---|---|
| 22 | `github-actions-mutable-action-tag` | 全部 action 钉 commit SHA |
| 5 | `allow-privilege-escalation-no-securitycontext` | 8 个容器(含 initContainer)加 `allowPrivilegeEscalation: false` + `capabilities.drop: [ALL]` |
| 1 | `run-shell-injection`(`release.yml`) | `${{ }}` 改走 `env:` + `"$VAR"` |
| 1 | `rust.lang.security.args.args` | 定向 `nosemgrep` + 理由 |

其中两条是**真漏洞, 不是误报**:

1. `release.yml` 把 `${{ github.ref_name }}` 直接插进 shell heredoc。`${{ }}` 是
   **文本替换, 发生在 shell 解析之前**, 所以标签名里的 `$(...)`/反引号会被当命令执行 ——
   而本 workflow 是 tag 触发, **标签名可任意构造**, 这是可达的攻击面。
2. `dtolnay/rust-toolchain@stable` 的 `@stable` 是**分支语义**(「用最新 stable Rust」)。
   直接把 ref 换成 SHA 会丢掉它、**把 Rust 版本永久冻结**。正解是
   **钉 action 的 SHA + 显式 `with: toolchain: stable`**, 两者不冲突。

#### 额外发现一条 semgrep 看不见的(人工复核 workflow 时看到)

`deploy-dev.yml` 的 Slack 通知把 `${{ github.event.head_commit.message }}` 裸插进
JSON payload。**提交信息完全由推送者控制**, 一个含 `"` 的信息就能闭合字符串、
注入伪造字段, 往 Slack 发任意内容(社工/钓鱼)。semgrep 报不出来是因为它只扫
`run:` 块, 不扫 `with:`。

**处置是「删掉不可信输入」而不是「转义它」。** 转义做得到(前一步 `jq` 生成合法
JSON, 再用 `payload: |` 块标量传回), 但那条路要同时躲两个坑: jq 输出的 JSON 以
`{` 开头, 直接写进 `with: payload:` 会被 YAML 当**流式映射**而非字符串; 而
GitHub 是**先做文本替换再解析 YAML**, 替换结果必须同时对 JSON 和 YAML 安全。
现改为发 run 链接 —— `github.server_url` / `github.repository` /
`github.run_id` 都不受提交者控制, **无需任何转义**。

**行为变化**: 通知不再显示提交信息, 改为给 run 链接(点进去能看到)。若日后要
恢复显示, 应走上面那条 jq + 块标量的路子, **不要改回裸插**。

> 这里值得记一笔的是**过程**: 我第一版改成了 `toJSON()`, 自审时才发现它是错的 ——
> `toJSON()` 返回的是**带引号的 JSON 字符串**, 嵌进已有引号里会得到
> `"text": "...prefix "hello" suffix"`, JSON 直接坏掉。`toJSON` 只能用在
> 「整个值」的位置。加上 §1.24 里的 pipefail 回归, 这条工作线上一共出现过
> **三次「想当然的修复」**, 两次是自己读代码时抓到的, 一次是实跑抓到的。
> 与其再叠一层精巧, 不如把危险输入拿掉。

#### `nosemgrep` 只对**紧邻的下一行**生效 —— 靠隔离探针才发现

抑制 `args` 那条时, 我把 `// nosemgrep:` 写在 5 行**说明注释的上方**, 扫描仍报出。
隔离探针(同目录三种写法 + **一个不抑制的对照组**)结论:

| 写法 | 结果 |
|---|---|
| A: 注释在上一行 | 已抑制 |
| B: 注释在同行行尾 | 已抑制 |
| C: **不抑制(对照组)** | **仍报出** |

对照组是关键: 若三种写法都不报, 就无法区分「抑制生效」与「规则没触发」——
**一个没有对照组的抑制实验, 证明不了任何事**。最终把 `nosemgrep` 挪到紧邻代码处。

#### 一条必须写明的判据修正: `end-action` 只对 `uses:` 步骤有意义

本节开头用「semgrep 段没有 `end-action` 结论行」当作崩溃的证据 —— 那是**当时**的
step 类型(`uses:`)下才成立的判据。

替换成 `run:` 之后, **该 step 同样没有 `end-action` 行, 但这是正常的**:
GitHub Actions 只为 `uses:` 步骤发 `end-action`, `run:` 步骤一律不发。
若沿用旧判据, 会在修好之后继续把「没有 end-action」读成崩溃, 或者反过来在
真出问题时误判为正常。

**换 step 类型就得换判据。** 现在判「semgrep 是否正常跑完」看的是:

1. 该 job 实际执行过的 step 名字里**有** `semgrep`(`run:`/`uses:` 都适用, 最可靠);
2. semgrep **自己的结论行**且数字对得上: `Ran 725 rules on 358 files: 0 findings`;
3. 紧随其后的 `ps1 encoding lint` 照常执行(证明前一步没有中断整个 job)。

#### 验证(不是推断)

- **门禁有牙**: 故意不排除那 12 条 → **exit=2**; 换成 CI 同款命令 → **exit=0**。
  两条都是实跑, 不是推理。
- 修完后 CI 同款命令: **725 条规则 / 358 个文件 / 0 finding / exit=0**。
- 本地五道门: fmt exit 0、ps1lint exit 0(12 个 .ps1 全 OK)。
- 3 个 workflow YAML 与 5 个 k8s manifest 均用 PyYAML 解析通过; 8 个容器
  的 `securityContext` 用脚本逐个断言存在(不是靠肉眼看)。

#### 未验证的部分(必须写明)

k8s 那 5 处 `securityContext` **没有在真集群上 apply 过** —— F-2/F-3 的部署端到端
本来就没跑通(K3s API 曾超时), 且 Docker daemon 现已停掉。故本次只声称
「YAML 结构正确、字段已就位」, **不声称「已验证不破坏现有工作负载」**。
`capabilities.drop: [ALL]` 对 postgres/nats/valkey 官方镜像的兼容性同理未实测。

### 1.25 三个镜像**一个都构建不出来** —— 首次真构建才暴露 (2026-10-04 已修)

长期挂着「镜像实际构建未验证」这条。起因是网络: 本机代理 10808 下线, 但
**Docker Hub 直连是通的** —— 于是这个卡了很久的 blocker 反而先解开了。
`docker pull rust:1.98.1-slim-bookworm` 成功, 于是第一次真正跑了 `docker build`。

结果: **三个 Dockerfile 全部失败或产出错误的东西。**

#### 1. `im-gateway.Dockerfile` 缺 `libprotobuf-dev` —— CI 绿而 Docker 红

```
protoc failed: google/protobuf/empty.proto: File not found.
google/protobuf/timestamp.proto: File not found.
```

`core.proto` import 了两个 well-known types, 而 `build.rs` 只给了 `&["proto"]`
作 include 路径、**没设 `PROTOC_INCLUDE`**, 完全依赖系统那份。

**关键在于两个发行版的打包不同**:

| 发行版 | `protobuf-compiler` 是否带 `/usr/include/google/protobuf/*.proto` |
|---|---|
| Ubuntu | **带**(顺带提供) |
| Debian | **不带**(在 `libprotobuf-dev` 里) |

CI 跑在 ubuntu-latest 上, 所以**一直是绿的**; 而镜像基础镜像是
`debian:bookworm-slim`, 只有 Docker 构建会炸。同一个 `build.rs`, 两条路径两种结果。

**判别式: 「本机能编」和「CI 能编」都不能替代「镜像能编」** —— 它们连
发行版都不是同一个。凡是「构建步骤依赖系统预置文件」的地方, 都要问一句
**目标基础镜像里到底有没有**。

#### 2. `docker build --check` **抓不到上面这个错**

三个文件 `--check` 全部 `no warnings found`。BuildKit 的 linter 检查的是
Maintainer/secret/ARG 那一类**惯例问题**, 不验证「COPY 的源在构建产物里是否存在」。
故**静态检查通过 ≠ 能构建**, 容器这条只能靠真构建。

#### 3. `im-core.Dockerfile` 叠了三重错

原文是:

```dockerfile
COPY --from=builder /build/target/release/im-core /usr/local/bin/im-core 2>/dev/null || \
     COPY --from=builder /build/target/release/im-gateway /usr/local/bin/im-core
```

1. **COPY 后面没有 shell** —— `2>/dev/null ||` 会被当成源路径的一部分, 语法非法。
2. **源文件永远不存在** —— `crates/im-core` **没有 `src/main.rs`**, 是纯库 crate
   (`lib.rs` + 无 `[[bin]]`), `cargo build -p im-core` 根本不产出可执行文件。
3. 就算能跑, 一个「叫 im-core、内容是 gateway」的二进制会**主动误导**排障的人。

改为显式构建 `im-gateway` 并**如实命名**; V1 真拆出独立进程、im-core 有了
真实 `main.rs` 之后再改。

#### 4. `im-migrate.Dockerfile` 与 `migrate-job.yaml` **直接矛盾**

`migrate-job.yaml` 从 2026-10-03 起就写着 `command: ["/app/im-migrate"]`,
并附注释「不用 `command: ["sqlx","migrate","run"]`: 那要求镜像里装 sqlx CLI」。

**而 Dockerfile 当时没跟着改** —— 它 `cargo install sqlx-cli`、ENTRYPOINT 指向
`sqlx`、把 `migrations/` 当独立目录 COPY 进去。两者对不上的后果很具体:
**Job 跑 `/app/im-migrate`, 镜像里根本没有这个文件** -> 容器直接 crash, 迁移永远跑不了。

同一个缺陷在 `im-gateway.yaml` 的 `run-migrations` initContainer 里也有一份:
`command: ["sqlx", ...]` + `IM_DATABASE_URL`(这个变量名**根本不存在**,
`AppConfig` 的字段是 `postgres_url`, 按 `IM_<UPPER_SNAKE>` 规则对应
`IM_POSTGRES_URL`)。

**2026-10-03 那次修复只改了 manifest, 漏了 Dockerfile 和另一个 manifest** ——
因为镜像从没被构建过, 没人能发现两处对不上。

#### 5. `secrets-template.yaml` 漏声明 `postgres-credentials`

`postgres.yaml` x3 + `im-gateway.yaml` x2 都在引用它, 模板里**只声明了
`im-core-env`**。缺它时 postgres 容器拿不到凭据 -> `CreateContainerConfigError`
-> Pod 卡在 `ContainerCreating`, 整个 namespace 起不来。已补上。

#### 6. 顺带: `EXPOSE 9000` 与实际不符

两个 Dockerfile 都 `EXPOSE 8080 9000`, 但本仓**没有任何 gRPC server**
(`grep -rn 9000 crates/` 无监听方; im-gateway.yaml 的 9000 Service 端口
早已删除)。留一个永不监听的端口只会让人以为有可连的 gRPC 端点, 已删。

#### 7. 修日志时, 我自己又引入一个 bug —— 靠实测抓住

`im-migrate` 的收尾日志写着「跑之前先报总数, 跑之后报**实际执行数**……两者不等
(本次没新增)时, 日志里能直接看出『这次是空跑』」, 但那行 SQL 是
`SELECT count(*) FROM _sqlx_migrations WHERE success` —— 数的是**表里全部成功行**,
与本次运行无关, 所以 `applied` 永远等于 `total`, **它声称要解决的困惑根本没被解决**。

改成「跑前记一次、跑后记一次、取差值」之后, **第一版守卫写错了**:

```sql
SELECT CASE WHEN to_regclass('public._sqlx_migrations') IS NULL THEN 0
            ELSE (SELECT count(*) FROM _sqlx_migrations WHERE success) END
```

看着能兼顾「新库还没这张表」, 实测在**全新库上报错**, 被 `unwrap_or(-1)` 吞成
-1, 于是日志打出「本次应用了 **8** 份」这种明显荒谬的数。

原因: **PG 在解析/计划阶段就解析子查询里的表名**, relation 不存在直接报错,
`CASE` 的短路求值没机会生效。表在不在的判断必须在**独立于该表**的查询里做。
改成两次查询。

**而新库正是首次安装的主路径** —— 只测「已迁移库的空跑」那条会完全错过它。
四个用例实测全对:

| 场景 | 结果 |
|---|---|
| 全新空库 | `applied_before=0 applied_now=7`, exit 0 |
| 立刻重跑 | `applied_before=7 applied_now=0`, exit 0 |
| 已迁移过的库 | `applied_now=0`, exit 0 |
| 手工建表的库 | **拒绝**, exit 1(`is_clean_target` 生效) |

#### 验证(全部实跑)

| 项 | 数字 |
|---|---|
| `im-gateway` 镜像 | **141MB, 构建 exit 0**; 跑起来 exit 78 并给出 `IM_POSTGRES_URL + IM_JWT_SIGNING_KEYS + IM_REFRESH_PEPPER` 提示; `id -u` = **1000**(非 root) |
| `im-migrate` 镜像 | **132MB, exit 0**; 内含 `/app/im-migrate`, **无 sqlx**, **无 /migrations**(迁移已内嵌) |
| `im-core` 镜像 | **141MB, exit 0**; 日志前缀 `[im-gateway]`, 如实反映内容 |
| 端到端迁移 | 新空库 `applied_now=7`; `_sqlx_migrations` 恰好 **7 行**无重复; 15 张表; 2 个触发器只建一次 |
| 幂等 | 第二次跑 exit 0, 行数不变 |
| `ldd` | 二进制只依赖 `libgcc_s` / `libm` / `libc` —— **镜像里装的 `libssl3` 其实用不到** |
| 本地门 | fmt exit 0(修完新代码后); 5 个 k8s manifest + 3 个 workflow YAML 解析通过; 8 个容器 `securityContext` 逐个断言存在 |

#### 未验证

**两个 k8s 清单的 `securityContext` 没有在真集群上 apply 过**(F-2/F-3 部署端到端
仍未跑通), 只声称「YAML 结构正确、字段已就位」, **不声称不破坏现有工作负载**;
`capabilities.drop: [ALL]` 对 postgres/nats/valkey 官方镜像的兼容性同理未实测。

#### 顺带发现一处 CI 抓不到的矛盾, 需部署负责人定

`database-url` 这个键在 **`postgres-credentials`**(im-gateway 用)与
**`im-core-env`**(migrate-job 用)里**各有一份**。两处都得手工填, 改一处忘另一处
就会连不上库。**需要一个单一事实源。** 本次未擅自统一, 因为这取决于真实集群里
已经存在哪个 Secret。

---

### 1.26 aux-01 命名门禁是**又一个恒通过的占位符** —— 补真检查, 并记下 4 条真实规范偏离 (2026-10-04 新发现 · 文档 owner)

#### 缺陷本体: 门禁从不存在

`.github/workflows/ci.yml` 的 lint job 里有一个步骤就叫 `aux-01 naming check`,
body 只有一行:

```bash
# 占位,恒通过:本步骤不做任何自动检查,只 echo,不能当门禁看。
echo "placeholder: no automated aux-01 §J naming check is enforced here"
```

**注释自己就承认了它不是门禁**, 却一直挂在 CI 上。aux-01 §J「工具强制」表格
声明「命名违规 → CI 失败」, 而对本仓的 DB / proto / 文件命名层, 这句话一直是
**不成立的**。与 §1.24 的 semgrep 假绿灯是同一类缺陷: 步骤存在 ≠ 检查存在。

§J 原文指定的工具是 `clippy::naming` / `sqlfluff` / `buf lint` /
`openapi-spec-validator`, **一个都没接**。CI 的 clippy 跑的是
`-- -D warnings`, 不含 `naming` 组; sqlfluff / buf / openapi-validator 无踪。

#### 处置: 新增 `scripts/check-naming-convention.ps1`

覆盖 aux-01 中**能被机械判定且当前真实成立**的子集(每条都对应规范条目):

| 规范 | 检查内容 | 当前基线(手工核对, 非估值) |
|---|---|---|
| §D.1/D.2 | 迁移文件名 `<4位>_<snake>.sql`; 序号 0001..N 无断档无重复 | 7 个迁移, 0001..0007 |
| §D.3–D.7 | 表/列 snake_case; 索引 `idx_`/`uniq_`; 具名约束 `chk_`/`uniq_`; 触发器 `trg_` | 14 表 / 39 列 / 18 索引 / 6 约束 / 2 触发器 |
| §E.1–E.5 | proto package `im.<svc>.v<n>`; service/message/rpc PascalCase; 请求类型 `...Request`; 返回类型必须是本 proto 已声明的消息或 `google.protobuf.*` | 1 包 / 1 服务 / 31 消息 / 22 rpc |
| §F.1–F.3 | Rust 源文件名、模块目录、proto 文件名 snake_case | 95 个 .rs / 1 个 .proto / 10 个模块目录 |
| §I.1 | 任何 SQL 标识符不得叫 `status` | 0 处(合规) |
| §I.2 | 任何 SQL 对象名不得含 `room` / `chat` | 0 处(合规) |

**基线下限守卫**: 每个解析出的集合都有一个**手工核对过的下限**, 低于下限即
`exit 2` fail-closed。这不是多余的防御 —— 本脚本第一版的表解析器漏了
`IF NOT EXISTS`, 从 39 个列名里静默抽出 **0 个**, 而「0 违规」与「仓库干净」
在输出上完全无法区分。**一个总是静默通过的守卫比没有守卫更糟。**

下限也在第一次运行时抓到了我自己的错: 我把索引数下限凭印象写成 20, 实际是
**18** —— 全仓 19 行 `CREATE INDEX` 中有一行是注释掉的(`idx_messages_content_fts`,
全文检索预留)。差点为了让门禁变绿而把下限改成一个仍然错误的数字。

#### 判别力: 24 个变异用例, 0 失败

绿灯只证明「没报错」。为证明它**能发现**违规, 对仓库副本逐项注入:

- **18 项注入违规**全部被捕获并点名, 覆盖每个规则族: 表名 `DeviceSessions` /
  列名 `displayName` / 索引 `index_users_state`(缺前缀) / 具名约束
  `users_env_extid` / 触发器 `environments_before_update` /
  package `im.Core.v1` / message `tokenPair` / rpc `listMessages` /
  请求类型 `GetMeQuery` / 返回未声明的 `UserProfile` / 序号断档 / 序号重复 /
  `Message_Router.rs` / `IdentityService/` / `Core.proto` /
  `status` 列 / `chat_rooms` 表。
- **1 项对照组**(完全不改动)保持绿灯 —— 没有它就无法区分「门禁能发现违规」与
  「测试装置永远失败」。
- **2 项解析退化**(删掉 5 个迁移 / 删掉 3 个 rpc)返回 `exit 2` 而非假装通过。
- **2 项反例守卫**: `google.protobuf.Empty` 必须**放行**; `migrations/README.md`
  这类非编号文件不得被误报。把宽泛规则写进去很容易, 但把正常写法当违规就是永久
  误报。
- **1 项跨平台回归用例**: 见下「大小写敏感枚举」。

只有退出码不够 —— 用例 01 额外断言了**诊断文本本身**(`0003_Create_Friend_Requests.SQL`
必须出现在输出里), 因为一个门禁「因为错误的原因变红」和「变绿」一样无用。

#### aux-01 §J 规定的 clippy 命令**根本无法编译** (实测)

§J 原文写的是:

```
cargo clippy -- -D clippy::all -D clippy::pedantic -D clippy::naming
```

实跑结果:

```
error[E0602]: unknown lint: `clippy::naming`
  = help: did you mean: `clippy::panic`
  = note: `-D unknown-lints` implied by `-D warnings`
```

**`clippy::naming` 不是存在的 lint 组**。clippy 没有 `naming` 这个分组, 命名类
lint 是**逐个**的(`non_snake_case` / `non_camel_case_types` /
`non_upper_case_globals` / `upper_case_acronyms` / `module_name_repetitions` ...)。
而 `-D warnings` 会把 `unknown_lints` 升级为硬错误 —— **谁按 §J 原样接线, CI 会
直接编译失败**。故本次**没有**把 `-D clippy::naming` 加进 `ci.yml`。

#### Rust 命名层其实**已经被拦住了**(变异实测, 非推断)

不能因为 §J 的命令写错就以为 Rust 命名没人管。实测: 往
`crates/im-common/src/config.rs` 注入一个 `fn Badly_Named_Function_For_Mutation()`,
跑 CI 同款命令 `cargo clippy --workspace --all-targets --locked -- -D warnings`:

```
error: function `Badly_Named_Function_For_Mutation` should have a snake case name
CLIPPY_EXIT=101
VERDICT: -D warnings DOES reject the injected naming violation
```

源文件改前改后 SHA256 一致(`RESTORE_OK=True`), 未留下任何残留。
**结论: 命名 lint 里默认开启的那批(style 组)已被现有 `-D warnings` 覆盖,
本仓 Rust 标识符命名在 CI 里是有牙的。** §J 缺的是 `clippy::pedantic` 与
若干**未默认开启**的命名 lint, 而 `pedantic` 误报率高, 是否引入属规范 owner
决策, 不宜由门禁工作顺手接上。

#### 跨平台漏洞: 大小写敏感枚举 (Linux 上会漏)

自审发现: `Get-ChildItem -Filter '*.sql'` 在 **Windows 大小写不敏感、在 Linux
大小写敏感**。若迁移文件被改名成 `0003_....SQL`, 开发机上门禁能报出来, 而
**真正跑 CI 的 ubuntu runner 上却完全看不见它** —— 门禁会在最该拦的那台机器上
放行。已改为显式枚举目录并用 `-clike '*.sql'`(大小写敏感)判定, 同时把
「数字开头但不是 `.sql`」的文件单独报出来。修复后补了对应的回归用例(用例 22)。

同一次自审还改了 `Stop-Parse`: 它原先直接 `exit 2`, 把此前已收集的真实违规
**一并吞掉**, 只留一句误导性的「无法解析」。现在先打印已收集的违规再退出 ——
基线失败常常是前面某个问题的**后果**, 吞掉根因会把人引向错误的 bug。

#### 变异测试抓到的真缺陷(本门禁自己有 13 处检查是废的)

首轮 22 个用例红了 8 个。根因: **PowerShell 的 `-match` / `-notmatch` 默认忽略
大小写**, 于是

- `^[a-z][a-z0-9_]*\.rs$` 会**放行** `Message_Router.rs`
- `^[A-Z][A-Za-z0-9]*$` 会**放行** `tokenPair`
- `^im\.[a-z]...` 会**放行** `im.Core.v1`

13 处风格判断全部改用 `-cnotmatch` 后重跑, 22/22 通过。
(注: `[regex]::Match` 本身**是**大小写敏感的, 与 PowerShell 运算符不同 ——
这正是 D.1 用 `[regex]` 而其余用 `-cnotmatch` 的原因。)

#### 明确**不做**自动化、以及为什么(都是实测, 不是假设)

| 规范条目 | 为何不查 |
|---|---|
| §I「禁中文/日文标识符」 | 需要真正的词法分析(先剥注释与字符串字面量)。实测朴素正则得 8 处命中, **全部是中文注释散文**, 零真违规 —— 会对注释误报的检查不是检查。clippy 无对应 lint, 留作 review 职责。 |
| §E「REST 路径须带 `/v1/` 前缀」 | 实测本仓合法地同时存在 `/v1/friends` 与裸片段 `/friends`, 因为 actix 是 `web::scope("/v1")` 套 `web::scope("/friends")` 的**嵌套**。要还原成完整路径必须理解 App 树, 正则做不到。留作 review 职责。 |
| §D「索引名须拼出每个列名」 | 其自带示例是 `uniq_users_environment_id_external_identity`。实测现有 9 个索引全部缩写(`uniq_users_env_extid` / `uniq_messages_idem` / `idx_messages_conversation_seq` 等)。第一天就报 9 条, 故**只记录不阻断**(见下)。 |
| §D「表名复数」/ §6「新表须先在 §G 术语表登记」 | 复数需词库; §G 覆盖率实测 5/14 张表缺失。属文档 owner 职责。 |
| §B Rust 标识符细则 | 需 AST, 交给 `clippy::naming`(见下)。 |

#### 落档的 4 条真实规范偏离(**不阻断 CI, 需规范 owner 裁决**)

1. **`messages.state` 应为 `delivery_state`** —— aux-01 §G「投递状态」行的单源词
   是 `delivery_state`, 且「严禁同义词」列明确列出 `~~status, state, ack~~`。
   而 `migrations/0005` 的列名是 `state`, 取值集
   `('sent','delivered','read','recalled','deleted')` 与 §G 描述**逐字一致** ——
   即语义完全对, 只是名字撞上了 §G 保留给 `users.state` 的那个词。
   **改名 = schema + proto + Rust + wire 契约变更, 不是机械改名, 不擅自动。**
2. **9 个索引名未拼出列名**(§D), 清单见上表。改名同样触及迁移历史(§H 规定
   迁移「永远追加, 不改历史」)。
3. **5/14 张表未在 §G 术语表登记**: `friend_requests` / `friendships` /
   `dm_pairs` / `audit_logs` / `conversation_sequences`(部分)。§6 验收标准要求
   「新表必须先在 §G 术语表登记」。
4. **§E 响应类型不强制 `...Response` 后缀**: 5 个 rpc 用
   `google.protobuf.Empty`(protobuf 自带类型, aux-01 管不到), 5 个直接把领域
   类型返回(`SendMessage → Message`、`GetMe → User`、`ExchangeToken → TokenPair`、
   `CreateConversation`/`GetConversation → Conversation`)。强制后缀要改 5 个
   活 rpc 的 wire 契约。脚本因此**只检查返回类型确实存在**。

#### CI 实证 (run 37196505674, commit b699472)

绿灯不等于门禁跑过, 故按「读 step 列表 + 读工具自己的结论行」验收:

- 4/4 job success, 且**每个 job 的 step 都在实际执行**(按 step 名去重后:
  lint 11 / unit 11 / integration 12 / sast 12, 无跳过)。
- `aux-01 naming check` 在 **ubuntu runner 上真实运行**, 输出与本机**逐项一致**:
  7 迁移 / 14 表 / 39 列 / 18 索引 / 6 具名约束 / 2 触发器 / 1 包 / 1 服务 /
  31 消息 / 22 rpc / 95 个 .rs / 10 个模块目录。
  **这条同时闭合了上面「大小写敏感枚举」的跨平台疑虑** —— 若 Linux 侧枚举
  结果与 Windows 不同, 上述数字就会不一样。
- 同 run 的既有门禁一并复核: `semgrep` = `Ran 725 rules on 359 files: 0 findings.`;
  `aux-03` = `OK: registry, docs and usages agree (21 codes)`;
  `cargo audit` = 扫描 439 crates 无漏洞;
  `im-migrate` = `applied_before=0 applied_now=7 applied_total=7`;
  Unit **148 passed / 0 failed**; Integration **412 passed / 0 failed**。

#### 顺带确认: F-2/F-3 仍被真实阻塞, 且比想的更硬

`kubectl` 可用(v1.36.1), 但 kubeconfig 指向 `172.28.176.169:6443`,
**连接被主动拒绝 —— 集群根本不在跑**。更关键的是:
`kubectl apply --dry-run=client` **仍需要 API server**(要取 `/openapi/v2` 做
schema 校验), 报 `failed to download openapi`。所以**没有集群就连清单都用
kubectl 验不了**, `kustomize build` 只能验 YAML 结构、验不了字段合法性。
F-2/F-3 需要先起一个本地集群(k3d / kind), 属需批准装工具的范畴。

#### 位置

- `scripts/check-naming-convention.ps1`
- `.github/workflows/ci.yml` lint job 的 `aux-01 naming check` 步骤
- 变异测试装置在 `target/logs/mutation-test-naming-gate.ps1`(gitignored;
  它是验证装置, 不是仓库资产 —— 结论与用例清单已完整抄录在本节)

---

### 1.27 D-3 实装: 事件总线从「静默 no-op」变成真 JetStream (2026-10-04)

#### 缺口本体

`NatsEventPublisher` 一直是 stub: `connect()` 不发起连接, `publish()` 丢弃
全部事件并返回 `Ok(())`。aux-04 §B.4 把「转换必须 publish 事件供其他 pod
同步」写成**不变量**, 而这条不变量从未被满足。§1.19(2026-10-03)已把它从
「完全静默」改成「可见可测量」(`warn!` + `im_events_dropped_total`), 但
**功能本身没实装**。

本次实装。规范口径查证: `BasicDesign` 技术栈基线写「NATS JetStream」;
WBS D-3 写「EventPublisher NATS JetStream 真实实现」;
`ImplementationSpec §7.4.5` 写 `/* publish via NATS JetStream */`;
`deploy/k3s/dev/nats.yaml` 也传了 `--jetstream`。四处一致, 无歧义。

#### 三个刻意的设计决定

1. **发布等 JetStream ack, 但有界超时(3s)。** `Context::publish()` 返回的是
   要等服务端 ack 的 future, 不设上界的话 NATS 变慢会**顺着业务请求路径**
   传导成延迟尖峰 —— 而 `DetailedDesign §9.1` 明确要求「失败不阻塞 ack」。
2. **Stream 由程序幂等创建, 不依赖运维预置。** `nats.yaml` 只传了
   `--jetstream --store_dir=/data`, **没有**任何 stream 配置; 安装包的
   `install.sh` 也不建 stream。若假定 stream 已存在, 首次部署时每条事件都会
   拿到 `no responders` —— 又一次静默失效。故 `connect()` 调
   `create_or_update_stream`(内部先 update, `NotFound` 时 create)。
   **第二个 pod 起不来**是这条设计的主要风险, 故专门有幂等用例守着。
3. **`kind=stub` 与 `kind=nats` 拆成两种类型。** 此前两者都调
   `NatsEventPublisher::connect` —— 于是「我要 stub」的实现是「去连
   `nats://stub:4222」`, 而「我要 nats」也什么都不连。现在 `kind=nats`
   连不上会**启动失败**; `kind=stub` 真的不连任何东西。配了 NATS 却静默
   退化成 stub, 正是这个文件长期存在的那类缺陷。

#### `/metrics` 从 1 个计数拆成 3 个

| 指标 | 含义 | 处置 |
|---|---|---|
| `im_events_published_total` | 拿到 JetStream ack | 正常增长, 无需处理 |
| `im_events_publish_failed_total` | 超时 / 服务端拒绝 | **事件没出去, 需查 NATS** |
| `im_events_dropped_total` | `kind=stub`, 显式不投递 | 配置问题, 非故障 |

把 failed 与 dropped 混成一个数, 会让「NATS 挂了」与「本来就配了 stub」在
面板上长得一样 —— 于是真正的故障反而看不见, 这正是原设计要消灭的形态。
`/readyz` 里的 `"nats": "not_checked_stub_publisher"` 也随之改为
`"not_checked"`: D-3 落地后 publisher 已不再是 stub, 留着旧值就是一句假话。

#### ~~**诚实声明: DLQ 未实装**~~ → ✅ 已于 §1.31 实装

`DetailedDesign §9.1` 要求「失败不阻塞 ack 但**写入 DLQ**」。本节交付时(D-3,
2026-10-04)的失败路径是有界超时 / 服务端错误 → 计数 + `error!` + 返回
`Err(ServiceUnavailable)`, 即失败**可见、可测量**, 但**不可恢复**。

**2026-10-05 已实装**: 按 `aux-08` 加了重试(§C.3 的 3 次 / 500ms·1s·2s)与
独立 `IM_DLQ` stream(§D.3 留存 7 天), 失败事件写入 `dlq.event.<topic>`(§D.2
的 `DlqRecord` 逐字段)。见 **§1.31**。仍未做的部分是 `aux-08 §D.3` 的
PG `audit_logs` sink。

#### CI: `services.nats` **用不了**, 只能 `docker run`(两条实测理由)

1. **GitHub Actions 的 `services.<id>` 不支持传 command/args。** 而
   `nats:2.10-alpine` 的**默认配置既不开 JetStream 也不开 8222 monitoring**
   —— 实测容器内 `netstat -ltn` 只有 `:::4222` 在 LISTEN, 日志也只有
   `Listening for client connections on 0.0.0.0:4222`。没有 `--jetstream`,
   `create_or_update_stream` 必然失败。
2. 若同时保留一个默认配置的 service 条目, 它会与 `docker run` **抢 4222
   端口**, 后者 bind 失败。

故改为一个显式的 `start NATS (JetStream)` 步骤: `docker run ... --jetstream
--store_dir=/data -m 8222`, 并以 **8222 monitoring 可达**为就绪判据(不是
「容器起来了」), 30s 超时后打印 `docker logs` 再失败。

#### 自审捉到的 2 处**我自己引入的**缺陷

都是「只写不跑」必然漏掉的:

1. **CI healthcheck 探 8222, 而 8222 根本没开** —— 第一版用
   `services.nats` + 基于 8222 的 healthcheck。实跑容器才发现默认配置不开
   monitoring, 该 healthcheck **永远不通过**, job 会卡住。改为 `docker run`
   显式加 `-m 8222`(实测 `wget` 返回 200)。
2. **保留了占位 service 条目会与 `docker run` 抢端口** —— 第一版改用
   `docker run` 时, 我为了「让意图可读」留了个 service 声明, 那会造成端口
   冲突。已删除。

另修 2 处自己写错的 API 用法: `AppError::Internal` 的内层是
`anyhow::Error` 而非 `String`; `Stream::info` 是**私有字段**, 公开的是返回
Future 的 `info()` 方法。这两处都是编译期才发现的。

#### 判别力: 4 个用例 + 变异测试(对照组 / 守卫组 / 跳过组)

关键在于**决定性断言不查返回值, 而查服务端**: 断言的是「发完之后 JetStream
stream 的消息计数确实增加了」, 这比「订阅者收到了」更强(证明被**持久化**),
也比「publish 返回了 Ok」强得多(后者正是旧 stub 也能满足的)。

四组实跑结果:

| 组 | 条件 | exit | 结果 |
|---|---|---|---|
| 对照 | 未变异 + 有 NATS | 0 | 4 passed |
| **变异** | `publish()` 改回「计数 + 返回 Ok」 | **101** | **2 failed —— 抓到了** |
| **守卫** | `IM_REQUIRE_NATS=1` 但无 NATS | **101** | 3 failed —— panic 而非静默跳过 |
| 跳过 | 无 NATS 且未要求 | 0 | 4 passed(本机不该全红) |

源文件改前改后 SHA256 一致, 还原干净(`FINAL_RESTORE_OK=True`)。

#### 仍未做

- **DLQ**(见上, 诚实声明)。
- **`/readyz` 仍不检查 NATS**: D-3 落地后已具备检查能力, 但该端点拿不到
  publisher 实例, 故仍显式报 `not_checked` 而非编一个恒为真的字段。
  需要把 publisher 放进 `AppState` 才能补上。

#### 位置

- `crates/im-core/src/event/publisher.rs` (主体)
- `crates/im-core/tests/nats_publisher_integration.rs` (4 用例)
- `crates/im-gateway/src/main.rs` (按 kind 分派; `kind=nats` 现在会启动失败)
- `crates/im-gateway/src/health.rs` (3 个指标 + readyz 取值)
- `.github/workflows/ci.yml` (integration job 的 NATS 步骤)

### 1.28 OpenAPI 3.1 规范 + 漂移门禁 (2026-10-05 落地, 用户选 A)

#### 落地前的状态

全仓**没有任何** OpenAPI / AsyncAPI 文件。对外契约只有 `core.proto`, 接入方
必须读 `crates/im-gateway/src/http/*.rs` 的 DTO 才能知道: 有哪些端点、每个收
什么字段、成功返回什么、失败返回哪个错误码。proto 描述的是 gRPC 面, 与 HTTP
面不是同一套形状(例: proto 的 `User` 没有 HTTP 侧 `MeResponse` 刻意剔除
`password_hash` 的那层约束)。对「主要用于集成」的工具, 这是最硬的接入障碍。

#### 交付物

- `docs/api/openapi.json` —— OpenAPI **3.1.0**, 21 个 path / 24 个 operation /
  26 个 schema。请求与响应 schema 的字段集**逐个**取自 Rust 的 `serde` DTO,
  不取自 aux-13 的样例 JSON(`ImplementationSpec` 处于 `[PROTOCOL-FROZEN]`,
  样例与代码冲突处以代码为准)。
- `scripts/check-openapi.ps1` —— 漂移门禁, 接入 `ci.yml` 的 `sast` job。
- 字段来源与「刻意不返回的字段」逐条写进了 `me.rs` 的模块文档, 规范里的
  `description` 复述了同一批理由。

#### 门禁实际检查什么(五项)

1. **路由双向比对**: 从 `main.rs` / `http/mod.rs` / `ws/router.rs` 重新推导
   路由表, 与规范双向比对 —— 规范有而代码没有、代码有而规范没有, 都会红。
2. **错误码**: 规范 `x-error-codes` 里每个取值必须存在于 aux-03 §B(21 项),
   且 21 项中没有被任何 operation 漏引(有理由的豁免见下)。
3. **operationId** 存在且唯一。
4. **所有 `$ref` 可解析**; 禁止 `TODO` / `待定` / `example.com` 一类占位符。
5. **每个 bearer 保护的 operation 必须声明 503 且引用 `SERVICE_UNAVAILABLE`**。

第 5 项是门禁**自己发现**的真缺口, 不是预防性检查: `crates/im-gateway/src/
http/auth.rs:66` 在 token service 未配置时, 对**任何**已鉴权请求都返回
`json_response(ServiceUnavailable, ...)`。这条路径独立于各 handler, 原先
没有任何文档提到它。补之前 16 个受保护端点全部漏写 503。

#### `IDEMPOTENCY_CONFLICT` 为何豁免(不是漏检)

aux-03 第 93 行明写该码 REST 侧返 **HTTP 200** 并回带原 `message_id`,
WS 侧返 `ack.ok=true`; 同一文件第 167 行的样例代码直接
`throw new Error("internal: IDEMPOTENCY_CONFLICT on 200")`。也就是说客户端
**收到**这个码就意味着实现有 bug。把它写进某个 operation 的错误码列表,
等于文档化一个规范禁止的响应。豁免名单手工钉住并断言(条目数必须为 1),
且每项必须仍存在于 aux-03 —— 若 aux-03 把它改名, 门禁会红而不是默默放过。

#### 路由解析为什么用括号深度栈

actix 的 `scope("X")` 只在**它所在的括号组**内有效, 但源码是流式链式调用,
文本上看不出边界。用「最后一个 scope 胜出」的纯文本扫描会恰好把这件事做反:

```rust
.service(web::scope("/v1").configure(http::configure))   // /v1 在此闭合
.route("/healthz", web::get().to(health::healthz))        // 不该继承 /v1
```

所以实现为: 预先算出每个字符偏移处的括号深度, `scope("X")` 入栈时记下当前
深度, 当深度回落到该值**以下**时出栈。纯文本扫描会把这个 `/healthz` 归到
`/v1` 下, 而实际它是根级端点。

#### 判别力实测: 18 个变异用例 0 失败

| 组 | 数量 | 内容 |
|---|---|---|
| 对照组 | 1 | 未变异必须绿(证明红的是变异造成的) |
| 注入缺陷 | 12 | 删/加/改规范路径、删/改代码路由、换 HTTP 方法、把路由挪进别的 scope、重复 operationId、引用不存在的错误码、占位符、悬空 `$ref`、删 `x-error-codes`、删 503 行、出现第二个 `configure` 委托 |
| 解析退化 | 1 | JSON 坏掉必须 `exit 2` 而不是当成通过 |
| 反例守卫 | 3 | 注释里的不平衡括号、注释里的 `//`、长得像路由的字符串 —— 都**不得**让门禁变红 |

源文件改前改后 SHA256 三份全部一致, 还原干净。

#### 开发过程中门禁自身暴露的 4 个缺陷(均已修)

1. **括号深度只在 match 之间统计**, 而正则本身吞掉了 `scope(...)` 与
   `web::post()` 的括号, 导致 `.route(` 的左括号被算成多余右括号, 活着的
   scope 被提前弹出 —— `/auth` 的 5 条路由丢了 4 条前缀。改为全量统计。
2. **HTTP 方法用 400 字符窗口回溯查找**, 会匹配到**下一条**路由的动词,
   造成静默串号。改为单条正则同时捕获 path 与 verb。
3. **注释里的孤立 `)` 破坏括号平衡**(`http/mod.rs` 就有), 必须先中和注释;
   但第一版把字符串内容也一并清空, 导致委托检测读到 `scope("")` 找不到任何
   委托。最终版用单次左到右的 alternation: 块注释丢弃、行注释丢弃、
   字符串**保留内容**只把括号换成空格。
4. **aux-03 正则缺 `(?m)`**, `^` 只锚在字符串开头, 21 个错误码解析成 0。

外加一处自身笔误: 改 `x-error-codes` 读取方式时漏写 `$op = $specOps[$k]`,
导致 24 个 operation 全读成同一个(哈希表最后一个), 报出 17 个「未被引用」
的错误码。

#### 仍未做(诚实声明)

- **AsyncAPI 缺失**: WS 帧协议(`im_protocol::ws_frames`)无机器可读描述。
  `/v1/ws` 在 OpenAPI 里只登记了 101 握手与首帧 `auth` 的形状, 6 类业务帧
  未建模 —— OpenAPI 3.1 无原生 WebSocket 支持。
- **`content` 未按 `kind` 展开**: 各 `kind` 的载荷 schema 在服务端校验,
  但 `ImplementationSpec` 冻结且只给样例, 展开即等于发明协议, 故只声明为
  自由对象并在 `description` 里写明「服务端为准」。
- **`GET /v1/friends` 仍不在规范里**: 它在代码中本就未注册(aux-13 说返
  `repeated Friend`、proto 说 `repeated User`, 矛盾未裁决), 见 §1.16。
- **媒体端点缺失**: `POST /v1/media/presign` / `GET /v1/media/{id}` 依赖
  对象存储, 未落地, 故既不在代码也不在规范, 见 §1.16。

#### 位置

- `docs/api/openapi.json` (规范)
- `scripts/check-openapi.ps1` (门禁)
- `crates/im-gateway/src/http/openapi_contract.rs` (运行时契约测试)
- `.github/workflows/ci.yml` (sast job 的 `check OpenAPI drift` 步骤)

#### 第二层: 运行时契约测试(与静态门禁互补)

`check-openapi.ps1` 是**静态**的 —— 从源码正则推导路由表。它挡得住「改了
代码忘了改文档」, 但它本身是个解析器, 而解析器可能解析错。故补一层
**运行时**对拍: `crates/im-gateway/src/http/openapi_contract.rs` 真搭一个
actix App, 对规范里每条 (method, path) 发**真请求**, 让 actix 自己的匹配逻辑
回答「这条路由存在吗」。

判别式: `404 且 body 为空` = 路由未命中。依据是本仓形状事实 —— actix 未命中
返回 `404 + 空 body`, 而 handler 的一切错误(含 404)都走
`error_response.rs::json_response`, 它**总是** `.json(body)`。

该判别式会退化(有人给 actix 装自定义 404 页, 或某个 handler 返回裸
`NotFound().finish()`), 故 `undocumented_path_is_not_routed` 是它的报警器。

不需要数据库: 用 `connect_lazy` 指向连不上的端口, `AppState` 的 7 个 service
全都能构造而不碰网络; 刻意**不**复用 `test_support::e2e_pool()`(它连不上就静默
skip, 而 skip 在 libtest 眼里等于通过)。

**CI 验收 (run 37274616003, head 032cb70): 4/4 job success, Integration
427 passed / 0 failed**(基线 424 + 新增 3), 三个契约测试逐条 `ok`。

#### 该测试在 CI 上暴露的 4 个缺陷(全部已修, 过程记录)

子代理产出的初版编译都过不去, 且**其中两个的报错完全指错了地方**:

| # | 现象 | 真因 |
|---|---|---|
| 1 | `E0308: expected Vec<u8>, found Bytes` | `test::read_body` 返回 `web::Bytes` 而非 `Vec<u8>` |
| 2 | `the async keyword is missing from the function declaration`, 指向一个**函数体里没有 `.await`** 的同步函数 | 裸 `use actix_web::{test, ...}` 把宏命名空间也导入了, `#[test]` 解析到 `actix_web::test` 属性宏(它要求 `async fn`)。**全仓 15 个 .rs 里只有这一个文件这么做** |
| 3 | 24 条 operation **全部** 404, 连根级 `/healthz` 也不中 | 规范里 method 是小写, `Method::from_str("get")` 匹配不上标准方法时**不报错**, 而是造一个**小写的自定义 method**; 路由表注册的是 `GET` -> method 不匹配 |
| 4 | —— | 装配代码写成宏, `App::new()` 的类型参数没被钉住。改成与本仓 `friends.rs::tests::friends_app` 一致的带显式返回类型的 `fn` |

第 3 项另有一个教训值得单列: **真实漂移总是零星几条, 「24/24 全中」只可能是
探针侧或装配侧的系统性问题**。当时没把「全中」当信号, 先去查了 App 的类型
参数(第 4 项), 白走一轮。现已加断言: 若全部 operation 都未命中, 报错直接说
「这不是规范漂移, 而是本测试 App 没提供任何可命中的路由, 先怀疑本文件」。

同理, 第 2 项的定位靠的不是盯报错行, 而是**全仓对照** —— 找出唯一一个偏离
邻近惯例的写法。

---

### 1.29 WS 投递按会话索引 + /metrics 契约闭环 (2026-10-05)

#### 一、WS 广播: fan-out-then-filter → 按会话索引 (commit `fc3715d`)

用户把本项目定位为「主要用于集成, RUST 特有的性能设计要足够完善」。WS 广播
是热路径上最贵的一处: 旧实现是 `broadcast` 单通道 + **先发给所有人再在接收侧
过滤**, 于是每发一条消息的成本是 **O(全部在线连接)**, 而其中 99% 的工作
(序列化、拷贝、入队) 花在**马上要被丢弃**的帧上。

新实现把过滤提前到发送侧:

| 旧 | 新 |
|---|---|
| 单个 `broadcast` 通道 | per-connection `mpsc` 通道, `PER_CONN_CAPACITY = 128` |
| 无索引 | `Index { sinks, by_conversation: HashMap<ConversationId, HashSet<ConnId>>, all_authed }` |
| 收件人过滤在接收侧 | 投递索引是**主过滤**, 接收侧 `should_deliver` 保留为纵深防御 |
| 每帧序列化 N 次 | `to_string` **只做一次**, 结果包进 `Arc<str>` 共享 |
| `Undeliverable` 仍遍历后丢弃 | **立即返回 0**, 一次索引都不碰 |

成本降到 **O(实际收件人)**。

容量 128 是算出来的, 不是拍的: 1 万连接 × 1024 帧 × 约 40B ≈ **400MB**,
不可接受; 128 约 50MB, 且 tokio mpsc 按块惰性分配, 小容量不预付满额内存。

**一处诚实声明**: 索引与 `should_deliver` 读的是**同一份鉴权快照**, 所以两者
并存**救不了**「会话期间被移出群」—— 那个缺口仍需基于事件的成员变更通知
(见 §2 表内 `ws/hub.rs` 那行)。保留接收侧过滤的价值是纵深防御, 不是修复。

`Lagged` 消失后**丢帧会完全静默**(没有 broadcast 就没有 `RecvError::Lagged`),
故补 `im_ws_broadcast_dropped_total` 把这份可观测性捡回来; 另加
`im_ws_authenticated_connections`(区分「连上」与「鉴权通过」)与
`im_ws_indexed_conversations`(索引规模)。

**性能断言不用计时**: `large_fleet_does_not_fan_out_to_non_members` 建 1000 连接
/ 500 会话, 只投递给 2 人, 然后断言**其余 998 个通道里确实什么都没有**。
计时断言必然 flaky, 「通道是空的」是结构事实。

`detach` 收敛到 `ws_handler` 循环返回后的**唯一出口**, 而不是散在各 `return`
分支 —— 漏一处就是一条永久泄漏的索引项。

**CI 验收 (run 37276522198, head `fc3715d`): 4/4 job success, Integration
433 passed / 0 failed**, 8 条 hub 新用例与 e2e
`broadcast_reaches_other_members_but_never_a_non_member` 全 ok。

#### 二、**自捕**: `/metrics` 的 description 是一份无人看管的散文 (commit `8550995`)

上面加的 3 个指标要写进 `openapi.json` 的 `/metrics` description, 才能让集成方
知道它们存在。写的时候才意识到: **那份 description 是手写的散文, 逐字复制
`health.rs` 里的 `# HELP` 文本, 而没有任何东西守着它。**

两道既有门禁都看不见它:

- `check-openapi.ps1` 比对**路由表**与**错误码**, 不看散文
- `openapi_contract.rs` 覆盖的是 (method, path) 是否命中, 也不看散文

即: 以后每加一个指标就必然产生一次漂移(代码里有、规范里没有), 集成方按规范
接进来就少一个可用指标, 而**没有任何测试会红**。这与「规范里写了路由但代码没
注册」是同一类缺陷, 只是方向是**散文 → 代码**, 正落在两道门禁的盲区。

补第三道门(测试 4), 三条通路各配对照, 判别力靠「换掉被测物, 结果必须变」:

| 通路 | 阳性对照 | 反例 |
|---|---|---|
| 指标名集合**双向**比较 | `EXPECTED_METRIC_COUNT = 7` 基线 | 变异 A 少写 / B 多写 / C 改名 |
| example 的 `# HELP` **逐字**核对 | 未变异时必须全对 | 变异 D 只改一个字 |
| 抽取规则 | 2 条真实形态 | 4 条坏输入 |

三个设计点值得单列:

1. **双向而非单向**。只做「文档 ⊆ 实际」会漏掉「代码加了指标、规范没写」——
   而那恰好是集成方真正会踩的方向(规范是他们唯一的依据, 没写就等于不存在)。
2. **空集合恒等是这个设计最容易出的假绿灯**: 两边都空就「相等」。故计数基线
   断言刻意排在集合比较**之前**, 且变异用例先自证「未变异时是干净的」——
   否则「变异被抓」可能只是因为它本来就常红。
3. **变异守卫做成常驻测试, 不用完即扔**。「门禁在注入缺陷时会红」若只在本机
   跑一次就丢, 下次改动它照样能悄悄退化。变异全部作用在**从 `SPEC_JSON` 读出的
   真实字符串**上, 不另造样本 —— 造样本只能证明「比较函数对假数据成立」。

抽取规则的第 3 条(字符集限制)不是冗余: 真实 description 里同时存在
`` `IM_EVENT_PUBLISHER_KIND` ``(大写环境变量名)与
`` `GET /v1/conversations/{id}/messages` ``(含斜杠与花括号)。两条**不是**假想敌,
此刻就写在同一段散文里。

**刻意不做**: 不把 description 里的中文散文也做成逐字校验 —— 那是给人读的,
强行机器化会导致改个措辞就红, 开发者随即绕过测试, 那比漏检更糟。逐字校验只
施加在 example 的 `# HELP` 行上(那本就是代码的抄本)。同理, 排版变化(去掉
`(gauge)` / `(counter)` 标注)明确**不得**被判成漂移, 已写成反例。

#### 验证

- 静态门禁 `check-openapi.ps1` exit 0(code routes 24 / spec operations 24 /
  spec paths 21 / aux-03 codes 21 / operationIds 24)
- `cargo fmt --all -- --check` exit 0; 无 U+FFFD; 行尾未被改写
- **CI 验收 (run 37279300456, head `8550995`): 4/4 job success, Integration
  436 passed / 0 failed / 0 ignored**(基线 433 + 新增 3), 6 条契约测试逐条 `ok`:
  `every_documented_operation_routes` / `undocumented_path_is_not_routed` /
  `route_table_baseline` / `metrics_description_matches_the_live_exposition` /
  `metric_extraction_rejects_non_metrics_and_uppercase` /
  `metrics_gate_rejects_mutated_specs`
- 本机 cargo **未作为验证源**: 共享 target 被其他项目长期占住 package/build 锁,
  连续三轮单次编译 >4min 仍未出结果, 故以 CI 为权威

#### 仍未做(诚实声明)

- ~~**AsyncAPI 仍缺**~~ —— ✅ **已于 §1.30 交付**(AsyncAPI 3.1.0, 19 message /
  34 schema, 静态门禁 + 运行时契约双层)。
  附: 逐个数 aux-13 的小节标题 —— 客户端 §1.1.1–§1.1.8 共 **8** 类, 与
  `ClientFrame` 的 8 个变体 1:1 对齐; 服务端 §1.2.1–§1.2.12 共 **12 个小节**,
  但 `ack` 占了 3 个(成功 / 幂等冲突 / 真错误), 去重后是 **10 个不同帧类型**,
  与 `ServerFrame` 的 10 个变体 1:1 完全对应。
- §1.18 的 `/readyz` 仍**不检查 NATS**(publisher 不在 `AppState` 里),
  也不检查 Valkey(D-4 不存在)。

---

### 1.30 AsyncAPI 3.1.0: WS 帧协议首次有机器可读描述 + 双层漂移门禁 (2026-10-05)

#### 缺口本体

`openapi.json` 覆盖 HTTP 面, 但 WebSocket 面此前**完全没有**机器可读描述。
接入方要自己读 `im-protocol` 的 Rust 源码, 反推 8 类上行 / 10 类下行帧的 JSON
形状。OpenAPI 3.1 无原生 WebSocket 支持, `/v1/ws` 在规范里只登记了 101 握手
与首帧 `auth`。对一个定位为「主要用于集成」的产品, 这是最大的接入障碍。

#### 交付物

- `docs/api/asyncapi.json` —— AsyncAPI **3.1.0**, 1 channel / 2 operations /
  **19 message** / **34 schema**。
- `scripts/check-asyncapi.ps1` —— 静态门禁, 接入 `ci.yml` 的 sast job。
- `crates/im-protocol/tests/ws_frames_contract.rs` —— 运行时契约测试(8 个用例)。

字段的**名字 / 可空性 / 必填性全部逐个取自 serde 派生**, 不取自 aux-13 的样例
JSON(`ImplementationSpec` 处于 `[PROTOCOL-FROZEN]`, 样例与代码在 `content`
内层 `kind`、`ErrorBody.ts` 等多处冲突, 以代码为准)。与 `openapi.json` 同一口径。

#### 规范里最容易被写错、因而必须被机器核对的一条规则

serde 给 `Option<T>` 有**三种**行为, 产生三种不同的 JSON:

| Rust 写法 | wire 上的表现 |
|---|---|
| 无属性 | 恒序列化, `None` 时 `"f": null` |
| **仅** `#[serde(default)]` | **同样恒序列化** —— `default` 只影响反序列化 |
| `default + skip_serializing_if` | key **整个消失** |

中间那条最坑: 很多人以为 `default` 让字段可省略, 于是标成非必填 —— 那是在
描述一个服务端**永远不会发出**的形状, 且不产生任何运行期错误。故规范里
`required` 的含义被显式写成「**wire 上一定出现**」, 门禁第 3 项逐字段核对它。

#### 双层门禁: 静态从源码推导, 运行时真序列化

静态门禁从 **Rust 源码文本**读 `skip_serializing_if` 属性 —— 那是一条关于
serde **行为**的断言, 不是对本仓 serde 实际行为的观测。运行时契约测试把每个
变体用「所有 Option 取 None」构造, 真跑 `to_string`, 要求 key 集合**恰好**
等于规范声明的 `required` ∪ {tag}。

必须双向相等: 只断言「required 里的字段都出现了」会漏掉「本该 skip 却恒出现
为 `null`」的那一半。24 个变体(8 + 10 + 6)逐个覆盖。

#### 门禁判别力: 13 个变异用例 0 失败

1 对照组 + 8 注入缺陷(删 / 加字段、skip 字段标 required、恒序列化字段标可选、
改判别常量、从 operation 删 message、悬空 `$ref`、破坏 message 数基线)
+ 2 反例守卫(`auth_ok` 豁免仍放行; 注释里长得像变体的文本必须被忽略)
+ 2 解析退化(Rust 括号坏掉 / JSON 坏掉, 均 `exit 2` 而非当成通过)。
源文件改前改后 SHA256 一致。

#### 门禁**第一次跑**报了 52 条, 其中 51 条是门禁自己的 bug

这条值得单列, 因为它是最容易犯错的判断:

1. **全局 discriminator 索引在 `typing` 上撞车**。`ClientFrame::Typing` 与
   `ServerFrame::Typing` 的 `properties.type.const` 都是 `"typing"`, 单个字典
   里一条静默覆盖另一条 —— 19 个 schema 被索引成 18 个, 然后门禁把上行的
   `typing` 拿去比对下行 schema 的字段。改为**按方向分别建索引**, 来源是
   operation 的 message 列表。
2. **字段正则的终止符只认逗号**。单行变体
   (`RecallMessage { req_id: Uuid, message_id: Uuid }`)的**最后一个**字段没有
   逗号 —— 那个逗号属于变体列表。改为「逗号**或** body 结束」的分支。
3. **把 serde tag 当成普通字段**。`type` 是 `#[serde(tag = "type")]` 注入的,
   不是结构体字段, 却拿去和 Rust 字段表比对 `required`。

**真实漂移总是零星几条; 一次跑出几十条, 几乎必然是门禁自己的解析或索引逻辑
出了问题。** 若照着违规逐条改规范, 会把一份对的规范改坏。

#### 同一处 `typing` 碰撞在两个独立实现里各犯一次

PowerShell 门禁与 Rust 运行时测试是两份独立代码, 各自踩了同一个坑。这说明
它是**协议形状里的真实歧义**, 不是某个语言的怪癖, 值得当设计问题修
(按方向分索引) 而不是随手补。

#### 首帧 `auth`: 规范是对的, 验证对象选错了

CI 首次运行(run 37283006605)Integration 红, 两条失败**都是测试自身缺陷**:

- 报错「规范少要求了一个字段」: 规范把 `ClientAuthFrame.req_id` 标成可选, 而
  序列化 `im_protocol::ClientFrame::Auth` 必然带 `req_id`。
- **规范是对的。** 生产路径 `handler.rs:331` 走私有的 `AuthFrame`
  (Deserialize-only, `req_id` 是 `#[serde(default)] Option<Uuid>`), **从不用**
  `im_protocol::ClientFrame::Auth` 解析首帧。同一 wire 形状, 两个 Rust 类型,
  只有一个是活的。第一版拿 A 类型的输出去验 B 类型的文档。

**没有豁免。** 豁免等于 auth 的任何东西都不再被验证, 而它恰恰是最容易漂的
一帧(裸 `json!` 构造, 不受类型系统保护)。改为单独一条用例, 把差异**锁成三条
断言**: ① 本枚举确实恒发 `req_id` ② 规范确实标它可选且仍记为属性
③ 规范的 description **必须点名** `AuthFrame`。任一被改而另一方没跟上就红。

静态门禁里也有一条对应例外(`$RequiredFieldExceptions`), 同样写明理由并
**断言条目数为 1**(照 §1.28 里 `IDEMPOTENCY_CONFLICT` 的先例)。

#### 结构合法性由官方工具独立验证

用官方 `@asyncapi/parser` 实测: 声明 3.0.0 时有 1 条 warning(建议升 3.1.0),
改为 **3.1.0** 后**零错误零警告**, 且 1 channel / 2 operations / 19 messages /
34 schema 的结构与 3.0.0 完全一致(本仓用到的 3.1.0 特性集是 3.0.0 的超集,
无需改结构)。对「商业产品标准」的交付物, 声明最新版本优于声明次新版本。

**两个独立工具**(本仓门禁 + 官方 parser)对同一份文档给出一致的结构判断 ——
互为对照, 不是单方自证。

#### 规范里显式记录的已知偏差(一律不擅自修)

- `auth_ok` **不在** `ServerFrame` 枚举里, 由 `handler.rs:381` 的裸
  `serde_json::json!` 构造, aux-13 §1.2 也未定义。按代码事实登记, 标
  `x-im-spec-status: undeclared`, **不**裁决它是否本应是 `connected`。
- 5 个下行帧登记但标 `x-im-implementation-status: reserved-not-emitted`
  (`connected` / `message_edited` / `reaction_added` / `presence_update` /
  `force_disconnect`)—— 让接入方知道协议预留了这些形状, 而不是让人去等它们。
- **REST 与 WS 的错误信封字段集不同**: REST 有 `conversation_id` 无 `details`
  (`error_response.rs::json_response`), WS 反之(`ErrorBody`)。规范里给了对照表。
- `message_new.reactions` 当前恒为 `[]`(`hub.rs:423` 硬编码)。
- 未知字段被静默忽略(全库无 `deny_unknown_fields`), 故所有 schema 的
  `additionalProperties` 显式为 **`true`** —— 标 `false` 会让严格校验的客户端
  拒掉服务端实际接受的帧, 那比不校验更坏。
- 上行 `send_message.kind` 与内层 `content.kind` **互不校验**, 客户端可发一对
  不一致的值且不被拒; 下行会把这对不一致一起广播。规范按代码事实记录, 不收紧。

#### 顺带纠正的一处错数

`ws_frames.rs:3` 的模块文档写「aux-13 §1.2 服务端 11 类」。逐个数小节标题:
§1.2.1–§1.2.12 共 12 个小节, `ack` 占 3 个, 去重后 **10 个不同帧类型**, 与枚举
10 个变体 1:1 对应。已改并写明推导。**别把「11」理解成「缺 1 类」**: aux-13
**没有**定义已读回执下行帧, 该需求来自另一份文档(aux-04 §B.4), 属规范级遗漏。

#### 验证

- `check-asyncapi.ps1` exit 0(client 8 / server 11 discriminator, 106 个 `$ref`
  全部可解析, 19 message / 34 schema)
- 变异测试 13/13 通过, SHA256 还原一致
- 官方 parser: `OK: valid AsyncAPI 3.1.0`, 零 warning
- **CI 验收 (run 37284406272, head `03549f2`): 4/4 job success, Integration
  **444 passed / 0 failed / 0 ignored**(基线 436 + 新增 8), 8 条新契约测试逐条
  `ok`; 门禁在 Linux 上输出 `OK: asyncapi.json matches the im-protocol serde
  definitions`; `lint-ps1-encoding` 判新脚本 `NO-BOM ascii-only`**
- 本机 cargo **未作为验证源**(共享 target 被其他项目占锁)

#### 位置

- `docs/api/asyncapi.json` (规范)
- `scripts/check-asyncapi.ps1` (静态门禁)
- `crates/im-protocol/tests/ws_frames_contract.rs` (运行时契约)
- `.github/workflows/ci.yml` (sast job 的 `check AsyncAPI drift` 步骤)

---

### 1.31 DLQ 实装: 事件发布失败从「永久丢失」变成「可恢复」(2026-10-05)

`DetailedDesign §9.1` 要求「提交事务后发布事件(事务外, 失败不阻塞 ack 但**写入
DLQ**)」, §1.27 交付 D-3 时这一半是缺的 —— 失败路径是有界超时 + `error!` +
返回 `Err(ServiceUnavailable)`, 即**可见、可测量, 但不可恢复**。本次补上。

按 `aux-08`:

- **§C.3** 重试 3 次, 退避 500ms / 1s / 2s
- **§D.1** 触发条件 = 重试耗尽
- **§D.2** `DlqRecord` 逐字段
- **§D.3** 独立 stream `IM_DLQ`(subject 过滤 `dlq.>`), 留存 7 天 / 上限 256MB

#### 一处**刻意的偏离**: 加了墙钟总预算(aux-08 没有这条)

照 §C.3 字面实现最坏要 **~15.5s**(3 次 x 3s ack 上界 + 3.5s 退避)。但
`publish()` 是被 `MessageService::send_message` 在 `tx.commit()` **之后内联
await** 的(`message/service.rs:232`), 于是这段耗时**直接变成客户端的响应
延迟** —— 大量客户端与中间代理会先超时, 「为了不丢事件」反而制造了「请求
超时」这个更常见的问题。

故引入 `RetryPolicy::total_budget`(5s):

| 场景 | 行为 |
|---|---|
| NATS 正常 | 第 1 次即成功, ~1ms, **零变化** |
| 连接被拒(快速失败) | 退避 500ms/1s/2s 后重试, ~3.5s -> 恢复 |
| NATS 变慢(每次撞上界) | 预算耗尽即转 DLQ, 硬钳在 5s 内 |

这个 **+2s**(相对实装前的 3s)是拿「不丢事件」换的, 属显式取舍, 不是疏漏。

#### 编排逻辑抽成**泛型函数**, 因为它本不该需要 NATS 才能测

`orchestrate_publish` 用两个闭包注入 IO, 于是「第一次就失败」「预算耗尽要转
DLQ」这类路径可以配 `tokio::time::pause()` **确定性**测试, 不碰 NATS ——
而这类路径恰恰是 CI 里最难稳定复现的。

7 个新用例(全部 `start_paused = true`): 首发成功 / 重试后成功 / 耗尽后恰好写
**一条** DLQ / 非 JSON 载荷退化为字符串且原始字节不丢 / 写 DLQ 失败被计数
且**不**被当成可恢复 / 预算装不下时提前转 DLQ / 预算充足时走满 3 档退避。

#### 三个字段恒为 `null`, 这不是偷懒

- **`error.stack`** —— 规范写「脱敏后」。本仓**没有**脱敏, 而把未脱敏 stack
  写进一个**留存 7 天、运维要读**的队列是净风险。宁可为空。
- **`context.trace_id` / `user_id` / `env_id`** —— `publish()` 在
  `tx.commit()` 之后调用, 那里已经没有请求上下文。**编造**一个值会让排障被
  错误信息带偏, 比空着更糟。

#### 诚实声明: `aux-08 §D.3` 的 PG sink **仍未实装**

§D.3 的 MVP 范围是「NATS DLQ subject **+** `audit_logs(action=dlq_record,
detail=JSONB)`)」, 当前**只做了前者**。后果是可命名的: NATS **整体**不可用时
连写 DLQ 也会失败, 那条事件仍然丢失。该情况由
`im_events_dlq_write_failed_total` 计数并 `error!` —— 即**丢失是可见的**, 但
它确实还是丢了。

补 PG sink 的阻力是实的: `migrations/0006` 的 `audit_logs` 有
`tenant_id UUID NOT NULL`, 且 `target_type` 是 6 值 CHECK(`user` /
`conversation` / `environment` / `extension` / `secret` / `system`), 而事件发布
路径**没有租户上下文**, 仓内也**无任何生产代码写 `audit_logs`**。补它要先决定
事件如何携带租户 —— 这不是实现方能独自拍的板。

#### 顺带: 指标输出一度在**说假话**

`4535dc2` 发现 `im_events_publish_failed_total` 的 `# HELP` 仍写着「**当前无
DLQ, 这些事件已丢失**」—— DLQ 实装后这句就是错的, 它会**主动**把运维引向
错误结论。上一道门禁(`metrics_description_matches_the_live_exposition`,
`8550995`)逐字比对 `# HELP` 却没抓到: 它比对的是 openapi.json 的 **`example`
字段**, 而那个 example 只抄了 **2 条**(共 9 个指标)—— 逐字校验覆盖 2/9, 门禁
是绿的但只守着五分之一。已补到 9 条, 并改为**由 `health.rs` 程序化生成**,
消灭「手抄一份 HELP 文本」这个根因。

顺带修正 `publisher.rs` 模块文档里一个过期行号(调用点是 `service.rs:232`,
原写 231)。

#### 顺带: 文档指着一个**不存在的节**

写本节时发现 §1.27 的「DLQ 未实装」声明已改成「见 §1.31」, 而 **§1.31 当时
并不存在** —— 一个悬空锚点。`publisher.rs` 模块文档里也有一处同样指向 §1.31
的引用。两者都等着这一节; 本节补上, 引用随之生效。

#### 验证

- **CI 验收 (run 37291027856, head `db03ce8`): 4/4 job success**, Integration
  **451 passed / 0 failed / 0 ignored**(基线 444 + 新增 7), Unit **163 passed**
- 前一版 run 37289975390 是红的且**只剩 1 条**失败, 根因是
  `std::time::Instant` 与 `tokio::time::sleep` **不是同一个时钟**: 后者推进
  tokio 虚拟时钟, 前者在 paused-time 下几乎不动, 于是预算**永远花不完**。
  生产下两者都是真实时钟所以线上不炸, 但「用 A 时钟度量、用 B 时钟操作」是
  等着出事的一处隐患, 且失效时**静默**。已改用 `tokio::time::Instant`
- `/metrics` 9 个指标的 `# HELP` 与 openapi.json example **逐字一致**(用独立
  复核脚本重新解析 `health.rs` 再比一次, 不复用生成器)
- `scripts/check-openapi.ps1` exit 0(24 routes / 24 ops / 21 aux-03 codes)
- `cargo fmt --all -- --check` exit 0
- 本机 cargo **未作为验证源**(共享 target 被其他项目占锁)

#### 位置

- `crates/im-core/src/event/publisher.rs` (全部 DLQ 实现 + 泛型编排)
- `crates/im-core/src/message/service.rs` (调用点, 3 处 outbox 文案已改)
- `crates/im-gateway/src/health.rs` (9 个指标)
- `docs/api/openapi.json` (`/metrics` 的 description + example)
- `migrations/0006_create_audit_logs.sql` (PG sink 目标表, **目前无人写**)

Commits: `492b624` · `81b6500` · `db03ce8` · `4535dc2`

---

### 1.32 `/readyz` 对一件正在发生的事说「一切正常」—— 它报的是恒定的 `not_checked` (2026-10-05)

`DetailedDesign §5`、`ImplementationSpec §3.1.7`、`deploy/k3s-dev-preflight-checklist.md`
**三处**都写着同一件事: 就绪检查覆盖 **PG/Valkey/NATS, 任一不可用返 503**。

而实现里写的是:

> | NATS | ❌ 不查 | ...但**这个端点拿不到 publisher 实例**, 所以无法探测它

这条理由在 2026-10-04(D-3)之后**已经不成立** —— publisher 早就真连 NATS 了,
只是没注入这个端点。于是 NATS 挂掉时的实际行为是:

- k8s 认为 Pod 仍然 ready, 继续送流量
- 每条领域事件重试 5s 后进 DLQ
- 集群**看起来完全健康**, 事件在一批批积压

`/readyz` 是编排器**唯一**的摘流量依据。它对一件正在发生的事说「正常」, 于是
集群没有可观测的降级信号。

#### 改动

- **`EventPublisher` 新增 `fn readiness(&self) -> PublisherReadiness`**,
  **刻意不给默认实现** —— 漏实现应当是编译错误, 而不是悄悄返回某个看起来正常
  的值。三个实现者(`Nats` / `Stub` / 测试里的 `Mock`)全部显式实现。
- **`PublisherReadiness` 四态枚举** 而非 `Result`: stub 若报成功, 集成方会以为
  NATS 正常而事件其实一条都没发出去。用枚举让每种状态都能**原样**出现在响应里。
- **`readiness_from_state()` 纯函数**: `Disconnected` 与 `Pending` 在 CI 里稳定
  构造不出来(前者需要一个真断开的连接, 后者只在重连窗口出现), 故把映射抽出来测。
- `main.rs` 注入 `web::Data<Arc<dyn EventPublisher>>`。**单独注入**而非塞进
  `AppState`: 探针只需要这一个字段, 让它依赖整个 AppState 会使任何构造
  AppState 的测试都得先凑齐 7 个 service。
- openapi.json 同步: description、200/503 两个 example、
  `ReadyResponse.checks.nats` 的 enum(原为 `["not_checked"]` 单值)。

#### 探针**不发网络请求**, 这是硬要求不是偏好

`readiness()` 读 `async_nats::Client` 内部的 `watch` channel, 是本地读。换成
「发消息等回包」会有两个问题: 给 NATS 加探针流量; NATS 假死(TCP 连着但不回包)
时把探针一起拖到超时 —— 而 `im-gateway.yaml` 的 readinessProbe **没写**
`timeoutSeconds`(默认 1s), 探针超时会累计 `failureThreshold`, 那与「判定为
不健康」在 k8s 眼里完全是两件事。

#### stub 不阻断 readiness, 但也不许报 `ok`

`IM_EVENT_PUBLISHER_KIND=stub` 时事件不投递是**配置决定**。报 `ok` 是撒谎(集成
方会以为事件在发), 报 503 是滥罚(每个本地开发部署都会永远不健康)。故报
`ok_stub_events_not_delivered`。

#### 补了一道此前**不存在**的门禁

`checks.nats` 的取值来自代码的 `as_wire_str()`, 规范里是
`ReadyResponse.checks.nats.enum`。此前这两者之间**没有任何东西**。集成方是按
**规范**写探针解析代码的 —— 规范少一个取值, 客户端就会在真实响应上走进未匹配
分支。新增两条断言把两侧集合钉成相等, 并用对照组排除「两侧都空」的空转。

门禁鉴别力**实测喂过 3 次失败输入**: 少一个取值 / 拼错一个字 / 多一个幽灵取值
→ 三次全红; 还原后 SHA256 与变异前逐字节一致, 2 passed。

> 第一条变异最初报「RED(0 failed)」—— 返回码非零却一条 FAILED 都没有。那是我
> 统计方式的假象, 单独重跑看完整输出后确认是 2 failed。**红灯也要问一句「我的
> 量具有效吗」**, 与绿灯同理。

#### 验证

- **CI 验收 (run 37295084289, head `83252bc`): 4/4 job success**, Integration
  **462 passed / 0 failed / 0 ignored**(451 + 新增 11), Unit **167 passed**
  (163 + 新增 4)
- 新增 11 个用例: im-core 映射 4 / health 路由级 3 / openapi 契约 2 /
  NATS 集成 2
- 本机 `cargo test -p im-core --lib` **50 passed**、`-p im-gateway --bins`
  **137 passed**(本机抢到了共享 target 锁, 故两条命令都**实跑**了而非只靠 CI)
- `cargo clippy --workspace --all-targets --locked -- -D warnings` **EXIT=0**
  (CI 同一条命令; 过程中抓到并修掉 2 处 `unused import` 与 1 处
  `unused variable`, 它们都会让 CI 变红)
- `scripts/check-openapi.ps1` exit 0(24 routes / 24 ops / 21 aux-03 codes)

#### 一个已拍板但**值得记下来**的取舍

NATS 不可用即 503 是规范原话, 本次照规范实现。但 §1.31 的 DLQ 落地后, NATS 已
从「事件丢失」降级为「事件积压」—— 用一个硬 503 把「降级但仍能服务」变成「全站
不可用」, 在可用性上是一次**可争议的**交换。

2026-10-05 已就此问过规范所有者, 选择**保持现状**(符合三处规范原文)。记在此处
是因为: 将来若有人问「NATS 抖动为什么导致全站 503」, 答案不是「没人想过」, 而是
「想过、问过、拍板了」。另一条更稳的形态是加宽限期(连续断线超过 N 秒才 503)、
或改为不断流只靠 `im_events_dlq_total` 告警, 两者都需要先改规范。

#### 位置

- `crates/im-core/src/event/publisher.rs` (`PublisherReadiness` +
  `readiness_from_state` + trait 方法)
- `crates/im-gateway/src/health.rs` (`readyz` 真探 NATS)
- `crates/im-gateway/src/main.rs` (`readiness_publisher` 注入)
- `crates/im-gateway/src/http/openapi_contract.rs` (新增门禁「测试 5」)
- `docs/api/openapi.json` (`/readyz` 契约)

Commit: `83252bc`

---

### 1.33 `/v1/ws` 声明了 4 个**不可能发生**的错误码, 而唯一真会发生的 400 没写 (2026-10-05)

#### 先更正一处我自己写错的记录

`04c2993` 的 commit message 里写「§1.26 记的『9 个索引名未拼出列名』是**伪
命题**, 实测 18 条 `CREATE INDEX` 都显式写了列」。**这个判断是错的**, 在此更正。

错在把两个不同的问题当成一个:

- `aux-01 §D` 要求的是**索引名**要拼出**每一个列名**(示例
  `uniq_users_environment_id_external_identity`)
- 我去核实的是**索引定义**有没有写列 —— 那是本来就有的东西, 与 §D 无关

按 §D 的字面口径重新逐条核(脚本解析 `migrations/` 全部具名约束与显式命名索引):

| 项 | 实测 |
|---|---|
| 具名约束 / 显式命名索引总数 | **21** |
| 名字漏拼了至少一个列名的 | **12** |

例: `uniq_users_env_extid UNIQUE (environment_id, external_identity)`、
`uniq_messages_idem UNIQUE (conversation_id, sender_id, idempotency_key)`、
`idx_conversations_environment_id ON conversations(environment_id, created_at DESC)`。

故 §1.26 的**实质结论成立**(名字确实缩写), 但**计数过时** —— 当时记的「9」是
更早一轮的统计口径。**未作任何代码改动**。

教训与本轮另两次同类: 量具/口径错了, 结论会看起来同样完整(那次是变异统计说
「RED(0 failed)」, 一次是备份脚本写了没跑导致还原静默失效)。**「我核实过了」
这句话本身需要说明核实的是哪个问题。**

#### 缺陷

`GET /v1/ws` 是 25 个 operation 里**唯一一个**「在 `x-error-codes` 列了错误码、
却一个对应响应形状都没给」的端点。它列的是 `UNAUTHORIZED` / `FORBIDDEN` /
`NOT_FOUND` / `RATE_LIMITED`, 而这 4 个码**在握手阶段一个都不会出现**:

| 码 | 为什么不可能 |
|---|---|
| `UNAUTHORIZED` / `FORBIDDEN` | 鉴权在**第一帧** `auth` 帧完成, 失败以 WS 帧回报(`error` / `ack.error`), 不是 HTTP 状态码 |
| `NOT_FOUND` | `/v1` scope **没有**统一鉴权层(鉴权逐路由加), 握手不查任何资源 |
| `RATE_LIMITED` | 仓内**没有任何限流实现**(见 §2 对应行) |

握手阶段**唯一**可能的非 101 响应是 **400** —— 请求不带 WebSocket upgrade 头时
`actix_ws::handle` 失败(`ws/handler.rs:170-176`)—— 它**没被写进规范**。

对集成方的后果很具体: 照规范写的 401/429 处理是永远不触发的死代码, 而真实会
拿到的 400 无据可查。

#### 改动

- `/v1/ws` 的 `responses` 加 `400`, 复用既有 `components/responses/BadRequest`
- description 写清那 4 个码是**带内**帧错误(保留它们 —— 它们是真的, 只是不
  作为 HTTP 状态码), 并说明 400 的唯一来源

#### 三条新断言(「测试 6」)

- `ws_handshake_declares_exactly_the_one_status_it_can_return` —— 非 101 响应
  集合必须**恰好**是 `{400}`
- `ws_declared_error_codes_are_marked_as_in_band` —— description 必须明确
  **否定**「这些码是 HTTP 状态码」这个读法, 而不只是列出它们
- `plain_get_on_the_ws_endpoint_really_returns_400` —— **运行时**打一次不带
  upgrade 头的 GET 断言真 400。前两条只是读规范; 若代码改成对非 upgrade 请求
  返 101 或 404, 规范会立刻变成假话。零 PG 依赖(`app_parts()` 用
  `connect_lazy` 死池, 握手在任何查询之前就失败)

#### 门禁鉴别力: 实测喂过 4 次失败输入

| 变异 | 结果 |
|---|---|
| 删掉 `400` 响应 | **RED** |
| `400` 换成 `403`(名字在, 指向变了) | **RED** |
| description 删掉「带内」 | **RED** |
| description 删掉「不会作为 HTTP 状态码返回」 | **RED** |
| 还原后 | **GREEN**(11 passed), SHA256 与变异前**逐字节一致** |

首轮两个变异被**跳过**: `"400": { "$ref": ... BadRequest }` 片段在 **16 个**
operation 里都出现, 不加锚点会改到别的端点。唯一命中断言把它拦住了; 改用
`/v1/ws` 独有的 `101` 行做锚点后 4 条全红。

#### 验证

- `cargo test -p im-gateway --bins` → **140 passed / 0 failed**(基线 137 + 3)
- `cargo fmt --all -- --check` exit 0
- `cargo clippy --workspace --all-targets --locked -- -D warnings` exit 0
- `scripts/check-openapi.ps1` exit 0(24 routes / 24 ops / 21 aux-03 codes)

#### 位置

- `docs/api/openapi.json` (`/v1/ws` 的 responses + description)
- `crates/im-gateway/src/http/openapi_contract.rs` (新增「测试 6」)

Commit: `04c2993`

---

### 1.34 DLQ 的 PG 长留存层: NATS 整体挂掉时事件**不再**永久丢失 (2026-10-06)

#### §1.31 留下的那半个缺口

§1.31 的 DLQ 只有 **NATS 一层**。`aux-08 §D.3` 要求两层, 且表格里 PG 那行的用途
写的是「**长留存**」—— 而 NATS DLQ 是 7 天 / 256MB。

更要紧的是: 缺了 PG 层, 「NATS 整体不可用」这个场景下**连死信都写不进去**,
事件真的永久没了。当时靠 `im_events_dlq_write_failed_total` 诚实计数, 即
**丢失可见, 但没接住**。

#### 为什么是**专表**而不是 `audit_logs`(2026-10-06 架构拍板)

`aux-08 §D.3` 的 MVP 原文确实是 `audit_logs(action=dlq_record, detail=JSONB)`,
但这条路**在 schema 上走不通**:

| 阻碍 | 位置 |
|---|---|
| `audit_logs.tenant_id UUID NOT NULL` | `migrations/0006:15` |
| `audit_logs.target_type` 6 值 CHECK(不含「事件」) | `migrations/0006:18` |

而事件发布路径**拿不到租户** —— `publish(topic, payload: &[u8])` 只有裸字节,
且在 `tx.commit()` 之后调用。凑出 `tenant_id` 只有两条路:

1. 改 `EventPublisher::publish` 签名把上下文带进去 → 触及**全部**调用点
2. 在**失败路径**上解析 `conversation_id` 再查 `environment → game → tenant`
   → 多一次 DB 往返, 且让 publisher 耦合仓储

两条都比重开一张表贵。故走专表, 这也正是 `aux-08 GAP-2` 计划的 V1+ 形态。

按 DB 三分类, 它是 **Transaction(事件流水) + Work(待重放队列)**: 表以追加为
主, 但带 `replayed_at` / `discarded_at` / `replay_attempts` 三列, 生命周期与
普通事件流水不同(完成后应清理, per `aux-08 §D.4` 的人工处置流程)。而
`audit_logs` 是「谁做了什么」的审计轨迹 —— 两种生命周期、两种保留期、两种读者。

#### 核心规则: 「可恢复」= **任一层接住**

```
nats   pg        判定
Ok     Ok/None   可恢复
Err    Ok        可恢复     <- PG 层存在的全部意义
Ok     Err       可恢复     <- 丢的是长留存副本, 事件仍在 NATS
Err    None      永久丢失
Err    Err       永久丢失
```

#### 测试抓到了我实现里的**两个真 bug**

真值表是这个改动的全部意义, 我第一版实现时把它写成了 `None | Some(Ok(())) =>
nats` 这样的合并分支 —— 结果 4 个用例**当场抓出两处判定反了**:

1. `Some(Ok(()))` 与「没配 PG 层」被合到同一支, 于是「PG 成功 + NATS 失败」
   判成不可恢复 —— 恰好是 PG 层**唯一重要**的那一格
2. 修完第 1 处后又把 `None` 算成「PG 成功」, 于是「没配 PG + NATS 挂了」被
   误判成可恢复

两处都是「看起来合理、只错一个格子」的形状, 而那一个格子**平时永远不触发** ——
不会在开发期冒烟, 只会在生产 NATS 真的挂掉时才暴露, 且暴露方式是「事件丢了但
没人知道为什么」。最终实现改成对 `(nats, pg)` 元组穷举的 5 个分支, 无
`unreachable!`。

除逐格外另加两个反例守卫: 两层都失败时错误串必须**同时**报出两边原因(只报
一边, 排障的人会去查那个健康的组件); 同样的 NATS 失败在 PG 成功/失败下必须
给出不同结论(否则说明 PG 的结果根本没参与判定)。

#### 指标: 三个计数器的语义必须分得开

| 指标 | 含义 | 级别 |
|---|---|---|
| `im_events_publish_failed_total` | 某次尝试没发出去(**含每次重试**) | — |
| `im_events_dlq_total` | 已落到可恢复的地方 | — |
| `im_events_dlq_write_failed_total` | **两层都没接住** → 事件真没了 | **P1** |
| `im_events_dlq_pg_write_failed_total` | PG 副本没写上 → 事件多半**还在** NATS | **P2** |

新加的 `dlq_pg_write_failed` 与 `dlq_write_failed` **必须分开**: 后者涨 = 正在
丢数据; 前者涨 = 备份没做上, 7 天后那条死信就查不到了。合成一个数就分不出
这两者 —— 而这恰是 `aux-08 §D.5` 告警阈值要分开的情况。`/metrics` 因此由 9 个
变成 **10** 个, `EXPECTED_METRIC_COUNT` 同步。

#### 顺带修的连带项

新增 migration 会打破既有断言, 一并更新(而不是让它们红着): `migration_smoke.rs`
7 → **8** 份 / 14 → **15** 张表 / 版本区间 `1..=8`; 两个测试名与文件头注释里的
旧数字也一并更正(名字里带过期数字本身就是一种文档漂移)。

`migration_smoke_pg.rs` 新增
`migration_0008_indexes_exist_and_spell_out_every_column` —— 它从
`pg_indexes.indexdef` **把列名抠出来**再逐个检查是否出现在索引名里。断言的
对象是 PG **实际建成**的索引, 不是我们希望的样子。

新表的 3 条索引刻意按 `aux-01 §D` **逐字拼出列名**。既有 12 个缩写的索引
(§1.33)重命名会触及迁移历史(`aux-01 §H` 规定「永远追加, 不改历史」), 不在本次
范围 —— 但新表不该再欠一笔。

#### 验证

- `cargo test -p im-core --lib` → **54 passed / 0 failed**(基线 50 + 新增 4)
- `cargo test -p im-gateway --bins` → **140 passed / 0 failed**
- `cargo test -p im-gateway --test migration_smoke` → **3 passed**
- `cargo clippy --workspace --all-targets --locked -- -D warnings` **EXIT=0**
  (过程中修掉 1 处 `unused_doc_comment` —— 真值表注释被我写进了函数体)
- `scripts/check-naming-convention.ps1` / `check-openapi.ps1` /
  `check-asyncapi.ps1` 均 exit 0

#### 首次 CI 红了 3 个 job, 4 条失败 —— 其中一条是**我按记忆写的解析器**

run 37336615992: Lint / Unit / Integration 三个 job 失败。四条根因:

| # | 失败 | 根因 |
|---|---|---|
| 1 | aux-01 命名门禁报 4 条 | `error_http_status` 违反 §I(`status` 保留给 `delivery_state` / `users.state`); 两个具名 CHECK 也不符合 §D.6 的 `chk_<table>_<column>` |
| 2 | `pg_sink_..._read_back` | 列名已改, 跟随更新 |
| 3 | `migrator_embeds_all_seven_migrations` | 7 → 8 |
| 4 | `migration_0008_indexes_exist_...` | **我的解析器** |

第 4 条值得单独记。我写的解析器是
`def.find(" ON public.dlq_records (")` —— 理由是「我只见过这个形状」。而真 PG 的
`pg_indexes.indexdef` 实际输出(run 37336615992 日志里的**原文**)是:

```
CREATE INDEX idx_dlq_records_original_task_failed_at
  ON public.dlq_records USING btree (original_task, failed_at DESC)
```

中间多一个 `USING btree`, 于是那条 find 匹配不到, 测试在真库上直接 panic。
**我按记忆写了格式, 没在真 PG 上核过** —— 与 §1.33 那次「核实的是相邻属性」
同源, 只是这次更浅: 连对象都没验对形状。

修法不只是把字符串改对: 解析器改成找 ` ON ` 再**配对括号**, 使
`USING btree` / `USING gin` / 未来的 `INCLUDE (...)` 都不影响; 并新增
`indexdef_columns_handle_the_real_pg_output` —— 用**上面那段 CI 日志里的原文**
当夹具, 锁住这个函数。该用例**不需要 PG**, 因为它守的恰恰是「解析器对得上真实
格式」这件事 —— 而那件事当初只在真库上才暴露。

命名那条(第 1 条)判定为**门禁正确、我错**: 全部 8 份 migration 里只有我这个用了
`status`, 而 §I 的规则就是「SQL 标识符不得含 `status` 词段」。改法是列名改为
`error_http_response_code`、约束名改为 `chk_dlq_records_error_http_response_code`;
**JSON/wire 侧仍是 `http_status`**(aux-08 §D.2 冻结), 故 Rust 的
`DlqError::http_status` 不改名, 只在列名上避开保留词。列名与 wire 字段名不一致
这一点已在 migration 注释里写明。

#### 位置

- `migrations/0008_create_dlq_records.sql` (表 + 3 索引 + 2 CHECK)
- `crates/im-core/src/event/publisher.rs` (`DlqSink` / `PgDlqSink` /
  `combine_dlq_results` / 双层写入)
- `crates/im-core/tests/dlq_pg_sink_integration.rs` (真 PG, 3 个)
- `crates/im-gateway/src/main.rs` (PG 层接线, 生产下总是 `Some`)
- `crates/im-gateway/src/health.rs` (10 个指标)

Commit: `2b73080`

---

## 2. 后续新增 (无字母编号, 2026-10-03 标注时未分配编号)

| 位置 | 缺口内容 (摘自代码注释) | 接线条件 / 依赖 |
|---|---|---|
| ~~`crates/im-gateway/src/health.rs:15` `readyz()` 路由尚未接线~~ | ~~F-4 (healthz/readyz) 路由尚未接线~~ | ✅ **路由早已注册**; 2026-10-03 **实装真实依赖检查**: 原实现**无条件返回 200**, k8s 会把连不上库的实例判为 ready 并把流量打过去。现查 PG(带 500ms 硬上界, 避免探针超时变 CrashLoop), 不可达返 503; NATS(stub)与 Valkey(D-4 不存在)显式声明「未检查」而非省略。见 §1.18 |
| `crates/im-gateway/src/http/state.rs:71` `AuthedUser.tenant_id` | 已聚合进鉴权上下文, 但现有 handler 尚未按租户过滤 | 多租户隔离随 **G-1 / V1** 落地 |
| ~~`crates/im-core/src/identity/service.rs` `server_secrets`~~ | ~~S2S token exchange 要按 environment 取 secret, 接线未完成~~ | ✅ **已接线** (2026-10-03): 被 `IdentityService::verify_server_signature` 真正使用, 见 §1.3 |
| `crates/im-gateway/src/http/auth_handlers.rs` `token_exchange` **nonce 防重放** | 协议有 `X-IM-Nonce`, 但服务端**未校验也未记录** —— 同一合法请求可在 ±300s 窗口内重放 | 需跨实例共享存储 → **WBS D-4 (Valkey)** 落地后接 |
| `crates/im-gateway/src/placeholder.rs` (整文件) | 10 个端点桩函数未被调用 | 该文件唯一职责就是存放未接线桩; 各端点随对应 WBS 项落地 |
| ~~`im_protocol::ServerFrame::Ack` **缺 error 载荷**~~ | ~~规范 (aux-13 §1.2.4) 规定的失败形状表达不了, 实现因此另造顶层 `{"type":"error",...}` 帧~~ | ✅ **已修复** (2026-10-03): `Ack` 补 `error: Option<ErrorBody>`, im-gateway 改发规范形状并删除 `WsErrorFrame`, im-testkit 补齐强类型版。见 §1.7.3 |
| ~~`im_protocol::ServerFrame` **缺 `message_new` 变体**~~ | ~~`ws_frames.rs` 模块文档称「aux-13 §1.2 服务端 11 类」, 实际枚举只有 9 个变体; aux-13 §1.2.5 定义的 `message_new`(新消息广播)**无对应变体** —— 即便 `send_message` 实装成功, 其他客户端也收不到广播~~ | ✅ **已修复** (2026-10-03): 补 `ServerFrame::MessageNew { message: WireMessage }`, 并实装 `ws::hub::WsHub` 广播中枢 + 成员过滤 + `handle_send_message` 接线。见 §1.10 |
| `ServerFrame::MessageEdited` / `ReactionAdded` **不可广播** | aux-13 §1.2.6 / §1.2.8 的 wire 形状**不带 `conversation_id`**, 广播中枢无从判断接收方是否该会话成员 —— 发给所有人即跨会话泄漏 | **需规范所有者给这两个帧补 `conversation_id`**, 属协议变更(同 §1.6 的 `[PROTOCOL-FROZEN]` 约束), 不由实现方拍板。在此之前「编辑消息」与「reaction」无法实时同步, 只能靠 REST 拉。见 §1.10 / §1.13 |
| `ServerFrame` **无已读回执变体** | aux-04 §B.4 转换表 line 239 要求 mark_read 的 effect 是「UPDATE last_read_sequence **+ fanout**」, 但 `ServerFrame` 里根本没有 read receipt 帧 | 需规范所有者新增该下行帧(或明确取消 fanout 要求)。`ConversationService::mark_read` 已返回 `bool`(是否真的推进), 补帧后据此判断「值没变就别广播」, 无需改接口。见 §1.12 |
| aux-13 只给**样例 JSON**, 不给**结构定义** | 连续三次撞到「样例里没写的字段, 实现方无权补」: `UNSUPPORTED_OPERATION`(§1.6) / `auth_ok`(§1.8.2) / 上面两个帧的 `conversation_id` | 建议规范改为给**字段表 + 可空性 + 取值域**, 而非单条样例。这是从根上消除此类缺口的唯一办法 |
| `ws/hub.rs` 成员关系**鉴权时快照一次** | 会话期间被移出会话, 仍会收到该会话广播, 直到该连接重连 | 需基于事件的成员变更通知, 随 **G-1 presence** 落地 |
| `ws/handler.rs` 鉴权成功回 **`{"type":"auth_ok"}`** | **`auth_ok` 不是 aux-13 §1.2 定义的任何帧类型**(§1.2.1 定义的是 `connected { session_id }`)。继 §1.6 的 `UNSUPPORTED_OPERATION` 之后**第二处凭空发明的 wire 帧** | **只记录不擅改**(见 §1.8.2): 改它变动客户端可见的 wire 形状, 且**无证据表明 `connected` 就是原意** —— 与 §1.7.3 的 `ack` 形状不同(那次有 testkit 证据, 方向无歧义)。需规范所有者确认该帧形状 |
| im-gateway 26 个 e2e 用例**连不上 PG 就静默通过** | 沿用 `auth_handlers` 约定 `let Some(p) = .. else { return }`。CI 若无 `DATABASE_URL`, 全部「跑过」与「没跑」**无法区分** —— 假绿灯向量 | 需 D-4 后在 CI 挂 PG service container; 或改「连不上即 fail」(会让无 PG 的本地全红, 属取舍, 未擅自改)。见 §1.15 |
| `GET /v1/friends` **aux-13 与 proto 互相矛盾** | aux-13 说 `repeated Friend {user_id, display_name, state, since}`, proto 说 `repeated User`(带 `external_identity_json` / `environment_id`)。按 `User` 实装 = 把**每个人的外部身份**发给所有能列好友的人; 按 `Friend` 实装 = 要新增类型并改 proto。仓储层另缺 cursor 支持 | **需规范所有者裁决**。不擅自选边。见 §1.16 |
| `POST /v1/media/presign` / `GET /v1/media/{id}` **无对象存储** | aux-13 §3.6 请求/响应样例齐全、proto 也有 message, 但预签名 URL 只能由真实 MinIO/S3 签发; 仓库无 media 模块、无 media 表、无对象存储配置。aux-06 line 442 亦写明「V1+ 实装」 | 依赖对象存储基础设施落地。返 mock URL 比不实现**更糟**(客户端拿着签不出东西的 URL 去 PUT, 失败难以诊断), 故不实装。即 WBS **G-2**, 状态 `Blocked` 等 **H-6**。见 §1.16 |
| ~~`deploy/k3s/dev/migrate-job.yaml` 引用**无法构建**的镜像~~ | ~~清单跑 `ghcr.io/yourorg/im1.0-im-migrate:latest` 执行 `sqlx migrate run`, 但仓内**没有** migrate 二进制, 也没有任何东西能构建该镜像~~ | ✅ **已修** (2026-10-03): 新增 `crates/im-migrate`, 用 `sqlx::migrate!` 把 7 份 SQL **编译期内嵌**进二进制(schema 与代码必然同版本); 清单命令改为 `/app/im-migrate`、环境变量更正为 `IM_POSTGRES_URL`; Dockerfile 加 `--target migrate`。**并首次由仓内代码在全新空库上真跑一遍** + 验证重跑幂等。见 §1.20 |
| 本机 `im_test` 库的 schema **不是用 `migrations/` 建的** | 手工建(或用更早版本 SQL 建), `_sqlx_migrations` 无有效记录。新 `im-migrate` 对它重放 0001, 报 `trigger "trg_environments_before_update" ... already exists`, 报错完全指不到原因; 更糟的是 sqlx 前缀 `while executing migration 1:` 让人误以为**迁移 1 写错了** | ✅ **已加迁移前检查** (2026-10-03): `im_migrate::is_clean_target` 判「业务表在 + 迁移历史空」时非 0 退出并给二选一处置指引。**该库本身未删**(它是本地测试依赖), 要么换新库, 要么备份后手工登记迁移历史。见 §1.21 |
| CI 两个 job **不设 `IM_POSTGRES_URL`** | 只有 `DATABASE_URL`, 而 `im-migrate` 的 e2e 找不到变量就 `return` —— 静默跳过, 「跑过」与「没跑」从输出上无法区分 | ✅ **已修** (2026-10-03): `test-unit` / `test-integration` 均补 `IM_POSTGRES_URL`。**根治仍待 D-4**: im-gateway 那 26 个 e2e 的同类问题未动(见上表 §1.15 行) |
| **Dockerfile 无法在本机构建验证** | 2026-10-03 实测: BuildKit 成功加载并解析 `Dockerfile`(语法有效), 但拉 `rust:1.98.1-slim-bookworm` 报 `registry-1.docker.io ... EOF` —— 与 GitHub 同一个代理问题, 且本地无 `rust:*` 缓存 | 与「推送本地 commit」同一个阻塞源: **本地代理掐断外网 registry**。代理恢复后跑 `docker build -t im1.0-im-gateway:local .` 即可验证。**本条不声称镜像可用** —— 只声称 Dockerfile 语法有效。见 §1.17 |
| **`NatsEventPublisher` 是静默 no-op** (D-3) | `publish()` 只发一条 `debug!`(默认不可见)并返回 `Ok(())` —— 每条 `im.message.{created,recalled,deleted}` 都被丢弃, 而 aux-04 §B.4「publish 事件供其他 pod 同步」这条**不变量**从未被满足。返回值/日志/指标三条渠道都指向「正常」 | 2026-10-03 **已改为可见**: `warn!` 每次 + `/metrics` 暴露 `im_events_dropped_total`。**D-3 本身仍未实装** —— 需可连的 NATS server 才能测(依赖 Docker Hub 恢复)。见 §1.19 |
| **WS 端点路径 `/ws` vs `/v1/ws`, 文档自相矛盾** | `aux-13` 的 wscat 样例与 `Observability.md §1.1.3` 写 `/ws`; `ImplementationSpec §3.2`、`138-dev-plan.md`、`ws/router.rs` 写 `/v1/ws`。**双层 scope 导致的 `/v1/ws/ws` 已修**(那两边都不是), 但这两者之间该选哪个仍未定 | **需规范所有者裁决**。本次按代码既有意图 + 仓内 REST 全在 `/v1` 下的惯例取 `/v1/ws`; 改判为 `/ws` 只需改 `ws/router.rs` 一行。见 §1.22 |
| im-gateway 26 个 e2e 仍**静默跳过** | ~~需 D-4 后在 CI 挂 PG service container~~ | ✅ **已根治** (2026-10-03): `IM_REQUIRE_PG=1` 时连不上 PG 直接 panic, 已接到 3 处连接池构造点(`test_support::e2e_pool` / `auth_handlers::e2e_pool` / `tests/migration_smoke_pg::pool`), CI 两个 job 均设上。带该开关跑全量: 410 passed / 0 failed, **零静默跳过**。见 §1.22 |

### 1.16 friends / media / me 共 8 个端点: 5 个已实装, 3 个卡在规范矛盾或缺基础设施 (2026-10-03)

`DetailedDesign §5` 列了 8 个端点, 但 §1.14 交付时它们一个都没有 REST 出口。
逐个核对规范后发现**它们的可实装程度差别很大** —— 差别不在难度, 在规范给了
多少形状:

| 端点 | 规范给了什么 | 结果 |
|---|---|---|
| `POST /v1/friends/requests` | body `{recipient_id}` + 全部分支状态码(aux-11 §4) | ✅ 实装 |
| `POST /v1/friends/requests/{id}/respond` | body `{accept}` + 204 + 三种错误码(aux-13 §3.7, `[PROTOCOL-FROZEN-PATCH]`) | ✅ 实装 |
| `POST /v1/friends/{id}/block` | 204(aux-08) | ✅ 实装 |
| `GET /v1/me` | proto `GetMe` 返回 `User` message | ✅ 实装 |
| `PATCH /v1/me` | proto `UpdateMeRequest` | ✅ 实装 |
| `GET /v1/friends` | **aux-13 与 proto 互相矛盾** | ❌ 未实装 |
| `POST /v1/media/presign` | 请求/响应样例齐全, 但**无对象存储** | ❌ 未实装 |
| `GET /v1/media/{id}` | 仅一行路径说明 | ❌ 未实装 |

#### 抓到两个**已实装 service 里的真 bug**

`RelationshipService` 早在 2026-08-23 就实装完毕, 但因为没有端点调用它,
**从未被任何测试跑过**。接上 e2e 后立刻暴露两处, 两处都与规范明确相反:

1. **`is_blocked` 的参数传反了**。仓储语义是 `is_blocked(user, target)` =
   「**user 被 target 屏蔽**」(SQL `WHERE user_id = target AND friend_id = user`,
   屏蔽者存在 `user_id` 列)。service 写的是 `is_blocked(recipient, sender)`,
   等于在问「recipient 被 sender 屏蔽」—— 方向正好反了。后果:
   **我拉黑的人照样能给我发好友申请**(该拒的没拒), 而我拉黑过的人我反而
   发不出申请(不该拒的拒了)。
2. **重复申请的错误码与 aux-11 §4 line 337-344 明确相反**。规范写
   「已有 pending → 409 `FRIEND_REQUEST_EXISTS`; 已有终态 → 409
   `INVALID_STATE_TRANSITION`」, 实现却是 pending 返 204 幂等成功、终态返
   `FRIEND_REQUEST_EXISTS`。两者状态码都是 409 所以**手测完全看不出问题**,
   只有断言到错误**码**才暴露。

> 教训与 §1.15 同源, 但多一层: **状态码对不等于实现对**。两个 bug 的
> HTTP 状态码恰好都是「看起来合理」的值(204 / 409), 只有比到 wire 码才发现
> 与规范相反。凡是规范点名了错误码的地方, 测试就必须断言码而不只是状态码。

##### ⚠️ 一处**客户端可见的行为变更**(需要下游知晓)

修正 #2 意味着 `POST /v1/friends/requests` 对**重复申请**的响应从 **204**
变成 **409 `FRIEND_REQUEST_EXISTS`**。任何依赖「重复申请返 204」的客户端
(把它当幂等重试用)会开始收到 409。

**为什么以规范为准**: 旧行为来自 2026-08-23 的一次实现决定(代码注释
「per 2026-08-23 P2-1 已知限制」), 而规定 409 的 aux-11 版本是 **v1.1.0,
日期 2026-09-01** —— 规范**更新**, 实现是过时的那一方。

**顺带暴露一个未解的产品问题**: `UNIQUE(env, sender, recipient)` **跨 state
阻断**(WBS B-3), 所以一旦被拒绝, 对方**永远无法重发**。aux-11 line 341 也
标注「被拒后无法重发, 待 PM 拍板」。本实现未擅自改动这个约束 —— 要放开得改
migration(部分唯一索引或引入 `superseded_by`)。若要维持 204 幂等, 同样得改
规范。**两条路都需要规范所有者决定, 未选边。**

#### `GET /v1/friends`: 两份规范互相矛盾, 故未实装

| 来源 | 说的是 |
|---|---|
| `aux-13 §2.5` | `ListFriendsResponse.friends` 是 `repeated **Friend**`, 而 `Friend { user_id, display_name, state, since }` |
| `crates/im-proto/proto/core.proto` | `ListFriendsResponse.friends` 是 `repeated **User**` |

两者**不可调和**: `User` 带 `external_identity_json` / `environment_id`, `Friend`
不带。这不是排版差异 —— 若按 `User` 实装, 好友列表会把**每个人的外部身份**
(provider + external_uid) 发给所有能列好友的人; 若按 `Friend` 实装, 就要
新增一个 `Friend` 类型并重写 proto。

另有第三处不齐: `PgFriendshipRepository::list_friends` 的 SQL 是
`SELECT friend_id FROM friendships ... LIMIT $2`, **cursor 形参根本没用上**
(注释自认「MVP: 未实现 cursor」), 既返回不了 `display_name`/`since`, 也产生
不了 `next_cursor`。所以即便矛盾解开, 仓储层也要重写。

**不擅自选边**: 选 `User` 就有跨用户身份泄漏, 选 `Friend` 就要动 proto。留待
规范所有者裁决。

#### media 两个端点: 缺的是基础设施, 不是代码

`aux-13 §3.6` 把 `POST /v1/media/presign` 的请求(`content_type` / `size_hint`)
与响应(`upload_url` / `media_id` / `expires_at`)样例给得很全, proto 也有
`PresignMediaRequest` / `PresignMediaResponse`。但**预签名 URL 只能由真实的
对象存储(MinIO/S3)签发**, 而仓库里没有 media 模块、没有 media 表、没有对象
存储配置。aux-06 line 442 也写明「V1+ 实装; MVP 返回 mock URL」。

返 mock URL 比不实现**更糟**: 客户端会拿着一个签不出东西的 URL 去 PUT, 得到
一个难以诊断的失败。故不实装, 记为依赖项。

#### `/me` 的一个必须显式拆开的陷阱

`im_core::identity::repository::User` **派生了 `Serialize`**, 且带有
`username` 与 `password_hash`(argon2id PHC 格式)。handler 里一句
`HttpResponse::Ok().json(user)` 就会把**密码哈希**发给客户端。
根因是 `User` 同时扮演「数据库行」与「对外表示」两个角色。

`MeResponse` 把两者拆开, 字段集取自 proto `User` message(规范自己给出的
「对外用户表示」定义), 并**排除** `password_hash` 与 `username`(后者 proto
也没有 —— 规范本身就把它排除在对外表示之外)。
`me_response_field_set_is_an_explicit_allowlist` 断言的是**整个键集合相等**,
所以「往 `User` 上加一个新列」不会静默泄漏, 但「往 `MeResponse` 上加一个字段」
必须同时改这个断言 —— 加字段是一个显式的、被审的动作。

变异验证: 把 `get_me` 改回 `.json(&user)`, 该用例立刻变红, 失败信息里直接
打印出泄漏的 `$argon2id$...` 与登录名。

#### 一处未裁决的 PATCH 语义

proto `UpdateMeRequest.display_name` 是 `optional`。缺省与「显式清空」在
`Option<String>` 上是同一个值, 分不开; 要区分需 JSON Merge Patch 的显式
`null`, 而 `aux-13` **没给 `/v1/me` 的 REST 样例**, 无从判断该端点要哪种。
本实现按「缺省 = 不改动」实装并写明。若规范所有者要显式 null 清空, 需改成
`Option<Option<String>>` 并在 handler 层区分两种输入 —— 那是 wire 变更。

---

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

**维护**: 本表为快照, 记录于 dev @ `a0e72f9` + 本次未提交改动 (2026-10-03)。
缺口被接线后请同步勾除本文档与代码注释两侧, 避免再次出现
「代码引用台账但台账不存在」或「台账有项但代码已删」的双向漂移。
