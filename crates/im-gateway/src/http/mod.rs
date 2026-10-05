//! REST API 路由注册
//!
//! 依据: ImplementationSpec §3.1 + aux-13 §3
//!
//! ## 注册范围 (per 132-wbs.md §5.3 + 2026-09-19 lane-backend-core 实装)
//! MVP Day 3: C-8 / C-10 / C-12 wired
//! MVP Day 4 (lane-backend-core-2): C-3..C-7 + C-11 wired
//! MVP Day 5 (lane-backend-core-3): C-9 wired
//! 留给后续: E-1..E-4 测试补齐, F-2 k3s namespace

pub mod auth;
pub mod auth_handlers;
pub mod conversations;
pub mod error_response;
pub mod friends;
pub mod me;
pub mod members;
pub mod message_actions;
pub mod messages;
#[cfg(test)]
pub mod openapi_contract;
pub mod state;
#[cfg(test)]
pub mod test_support;

use actix_web::web;

/// 注册 /v1 路由
pub fn configure(cfg: &mut web::ServiceConfig) {
    // 鉴权(5 个端点) — MVP Day 4 实装 C-3..C-7
    //   C-3 POST /auth/token/exchange → auth_handlers::token_exchange
    //   C-4 POST /auth/guest         → auth_handlers::guest
    //   C-5 POST /auth/refresh       → auth_handlers::refresh (per 138 §8 缺口 #8 fix 用 find_by_id)
    //   C-6 POST /auth/link          → auth_handlers::link_account
    //     (2026-10-03 更正: 本行原注释「待 UPDATE 实现, 当前返 InternalError」
    //      已过时 —— IdentityService::link_account 早已实装 guest→user 升级)
    //   C-7 POST /auth/logout        → auth_handlers::logout (2026-10-03 已实装:
    //     dsid claim 加入 TokenClaims, 此前本端点无条件报错, access token 无法吊销)
    cfg.service(
        actix_web::web::scope("/auth")
            .route(
                "/token/exchange",
                actix_web::web::post().to(auth_handlers::token_exchange),
            )
            .route("/guest", actix_web::web::post().to(auth_handlers::guest))
            .route(
                "/refresh",
                actix_web::web::post().to(auth_handlers::refresh),
            )
            .route(
                "/link",
                actix_web::web::post().to(auth_handlers::link_account),
            )
            .route("/logout", actix_web::web::post().to(auth_handlers::logout)),
    );

    // 会话(9) — MVP Day 3+5 实装 C-8 + C-9 + C-10
    //   C-9 messages POST/GET    → lane-backend-core-3 实装
    //   C-8 conversations POST   → 已有实装(创建 dm/group/channel)
    //   C-8 conversations GET    → 已有实装(列出当前用户所有会话)
    //   C-10 conversations/{id}/members GET → 已有实装
    //   2026-10-03: GET conversations/{id} 不再走 placeholder —— 它此前被标
    //     「out-of-scope」, 但 `ConversationService::get` 早已实装, 挂着
    //     placeholder 只会让一个**能做的端点**看起来做不了。
    //
    //   2026-10-03 新增 4 个消息动作端点(per BasicDesign §7 + DetailedDesign
    //   端点→需求映射表): edit / recall / reactions / read。此前这四项
    //   **只有 WS 帧**, REST 侧完全没有 —— 服务端 SDK / 后台任务 / 非 WS
    //   客户端无法触达。四个 handler 全部只做「解析 → service → 状态码」,
    //   业务规则与 WS 路径共用同一条 service 校验链, 不重写。
    cfg.service(
        actix_web::web::scope("/conversations")
            .route("", actix_web::web::post().to(conversations::create))
            .route("", actix_web::web::get().to(conversations::list))
            .route("/{id}", actix_web::web::get().to(conversations::get))
            .route(
                "/{id}/messages",
                actix_web::web::post().to(messages::send_message),
            )
            .route(
                "/{id}/messages",
                actix_web::web::get().to(messages::list_messages),
            )
            .route(
                "/{id}/messages/{msg_id}",
                actix_web::web::patch().to(message_actions::edit_message),
            )
            .route(
                "/{id}/messages/{msg_id}/recall",
                actix_web::web::post().to(message_actions::recall_message),
            )
            .route(
                "/{id}/messages/{msg_id}/reactions",
                actix_web::web::post().to(message_actions::add_reaction),
            )
            .route(
                "/{id}/read",
                actix_web::web::post().to(message_actions::mark_read),
            )
            .route("/{id}/members", actix_web::web::get().to(members::list)),
    );

    // 好友关系 (IM-REL-001 / MOD-FR-001) — 2026-10-03 实装
    //
    // 三个端点的成功响应**一律 204**: 已是好友与新建申请返回同一个东西
    // (proto 三个 RPC 全返 google.protobuf.Empty), 若给新建返 201+body,
    // 客户端就无法区分「首次创建」与「幂等重发」。依据见 friends.rs 模块文档。
    //
    // 注: `GET /v1/friends`(好友列表)**未**注册 —— aux-13 说返回
    // `repeated Friend`、proto 说 `repeated User`, 两者矛盾且仓储层没有
    // cursor 支持(只返回 friend_id)。凭空选一个就是发明 wire 形状。
    // 记在 docs/gap-ledger.md §1.16, 待规范所有者裁决。
    cfg.service(
        actix_web::web::scope("/friends")
            .route(
                "/requests",
                actix_web::web::post().to(friends::send_request),
            )
            .route(
                "/requests/{id}/respond",
                actix_web::web::post().to(friends::respond_request),
            )
            .route("/{id}/block", actix_web::web::post().to(friends::block)),
    );

    // 用户资料 (IM-ID-001) — 2026-10-03 实装
    //
    // 走 `MeResponse` 而非直接序列化 `im_core::User`: 后者派生了 Serialize
    // 且带 `password_hash`(argon2id), 直接 json() 会把哈希发给客户端。
    // 见 me.rs 模块文档的字段对照表。
    //
    // 注: `POST /v1/media/presign` 与 `GET /v1/media/{id}` **未**注册 ——
    // 二者需要对象存储(MinIO/S3)做预签名, 该基础设施尚未落地, 仓里没有
    // media 模块。记在 docs/gap-ledger.md §1.16。
    cfg.service(
        actix_web::web::scope("/me")
            .route("", actix_web::web::get().to(me::get_me))
            .route("", actix_web::web::patch().to(me::update_me)),
    );

    // WebSocket (C-11 driver) — MVP Day 4 实装
    //
    // 2026-10-03 修正: 原先是 `cfg.service(scope("/ws").configure(ws::router::configure))`,
    // 而 `ws::router::configure` 内部**已经自带** `scope("/ws")`。actix 的 scope
    // 是**嵌套**的(前缀逐层相加, 不合并), 于是实际路径变成 `/v1/ws/ws` ——
    // `ws/router.rs` 自己的文档与 `138-dev-plan.md` 都写的是 `/v1/ws`, 两者
    // 都不是。按规范接的客户端**根本连不上**, 且因全仓无任何测试真的发起一次
    // WS 连接而一直无人发现。
    //
    // 路径的定义权归 `ws::router`(它自带 scope 且文档写明 `/v1/ws`), 这里
    // **直接调用**而不再包一层。
    //
    // 注: 文档对 WS 路径本身有分歧 —— `aux-13` 的 wscat 样例与
    // `Observability.md §1.1.3` 写 `/ws`, `ImplementationSpec §3.2` 与
    // `138-dev-plan` 写 `/v1/ws`。此处按**代码既有意图 + 仓内 REST 全在 `/v1`
    // 下**的惯例取 `/v1/ws`; 若规范所有者定案为 `/ws`, 改 `ws/router.rs` 一行
    // 即可。记在 docs/gap-ledger.md §1.22。
    crate::ws::router::configure(cfg);
}
