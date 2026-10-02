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

## 2. 后续新增 (无字母编号, 2026-10-03 标注时未分配编号)

| 位置 | 缺口内容 (摘自代码注释) | 接线条件 / 依赖 |

| 位置 | 缺口内容 (摘自代码注释) | 接线条件 / 依赖 |
|---|---|---|
| `crates/im-gateway/src/health.rs:15` `readyz()` | F-4 (healthz/readyz) 路由尚未接线 | 依赖 **F-2 / F-3 (K3s 部署)**, 二者受 F-1 (Docker daemon 间歇性故障) 阻塞 |
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
| im-gateway 13 个 e2e 用例**连不上 PG 就静默通过** | 沿用 `auth_handlers` 约定 `let Some(p) = .. else { return }`。CI 若无 `DATABASE_URL`, 全部「跑过」与「没跑」**无法区分** —— 假绿灯向量 | 需 D-4 后在 CI 挂 PG service container; 或改「连不上即 fail」(会让无 PG 的本地全红, 属取舍, 未擅自改)。见 §1.15 |

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
