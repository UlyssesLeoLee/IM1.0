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
| **#F** | `src/ws/heartbeat.rs:101/110/118/125` `PingFrame` / `PongFrame` / `PingPongType` / `into_pong` | C-12 预留的 wire 镜像 | C-11 driver 决定走本 struct 还是 `im_protocol::ws_frames::ClientFrame`, 二选一 |
| **#G** | `src/ws/session.rs:95/102` `user_id()` / `environment_id()` | 鉴权后身份读取接口; 另有「同 user_id 多租户路由」 | C-11 driver 做 ForceDisconnect / 广播分发时按 user_id 定位 |
| **#H** | `src/ws/session.rs:104` `tick_heartbeat()` | 心跳 tick 返回 true 表示应主动 close | 30s background task 调用点待 C-11 driver 实装 |
| **#I** | `src/ws/session.rs:129/139` `WsInbound` / `handle_ping()` | C-11 driver 统一入站分派入口 | C-11 driver 实装后接线 |

> #D/#E/#F/#G/#H/#I **全部阻塞在 C-11 (WsSession + actix-ws driver 实装)**。
> 换句话说这 6 个缺口是同一个上游任务的下游, 接线 C-11 可一次性清掉。

### 1.1 #H 的严重性升级: 60s 无帧超时当前**根本不生效** (2026-10-03 复核)

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

**修法方向**(需 C-11 driver 重构, 且**端到端验证依赖 Docker/F-1**):
让超时信号能到达持有 `Session` 的主循环, 例如后台任务通过
`tokio::sync::oneshot` / `mpsc` 把超时事件发给主循环, 由主循环在
`tokio::select!` 中 `msg_stream.next()` 与超时分支之间二选一后 close;
或把 `interval` 直接搬进 `run_ws_loop`, 用 `select!` 同时等消息与 tick。
**在此之前不应声称"无帧超时已实现"**。

---

## 2. 后续新增 (无字母编号, 2026-10-03 标注时未分配编号)

| 位置 | 缺口内容 (摘自代码注释) | 接线条件 / 依赖 |
|---|---|---|
| `crates/im-gateway/src/health.rs:15` `readyz()` | F-4 (healthz/readyz) 路由尚未接线 | 依赖 **F-2 / F-3 (K3s 部署)**, 二者受 F-1 (Docker daemon 间歇性故障) 阻塞 |
| `crates/im-gateway/src/http/state.rs:71` `AuthedUser.tenant_id` | 已聚合进鉴权上下文, 但现有 handler 尚未按租户过滤 | 多租户隔离随 **G-1 / V1** 落地 |
| `crates/im-core/src/identity/service.rs:49` `server_secrets` | S2S token exchange 要按 environment 取 secret, 接线未完成 | **WBS C-3** 接线 |
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
