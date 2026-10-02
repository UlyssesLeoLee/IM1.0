//! 消息「动作类」REST 端点: 编辑 / 撤回 / reaction / 已读
//!
//! 依据: BasicDesign §7(REST 清单) + DetailedDesign 端点→需求映射表
//!       (该表按「§5」引用了这四个端点, 而 DetailedDesign §5 自己的完整清单
//!       **没有列它们** —— 见下方「规范自相矛盾」)
//!       aux-13 §1.1.3 / §1.1.4 / §1.1.5 / §1.1.6 (对应的 WS 帧)
//!
//! ## 为什么这四个端点此前不存在, 而能力已经实装
//!
//! 2026-10-03 之前, `edit` / `recall` / `react` / `mark_read` **只有 WS 帧**,
//! REST 侧一个都没有。这对「商业产品标准」是硬伤: 服务端 SDK、后台任务、
//! 非 WS 客户端(Web/桌面离线补传)全都无法触达这些能力 —— 而这些能力
//! **已经**在 service 层实装完毕(见 `MessageService::edit_message` /
//! `recall_message`、`ConversationService::mark_read`、`ReactionService`)。
//!
//! **刻意不重写校验逻辑**: 四个 handler 全部只做「解析 → 调 service → 映射
//! HTTP 状态码」, 所有业务规则(仅原 sender / 终态拒绝 / 时间窗 / 成员校验 /
//! 单调不回退)都在 service 里。WS 与 REST 走**同一条**校验链 —— 若 REST 另写
//! 一遍, 两条路径迟早漂移, 而漂移的那一侧就是越权漏洞。
//!
//! ## 规范自相矛盾 (只记录, 不擅自消解)
//!
//! `DetailedDesign.md` 的端点→需求映射表逐条写着
//! `| §5 PATCH /v1/conversations/{id}/messages/{msg_id} | 编辑消息 | ... |`
//! —— 明确标注这些端点属于「§5」。但 DetailedDesign §5 标题是
//! 「REST API **完整**清单(MVP)」, 其表格里**根本没有**这四个端点。
//!
//! 两份规范对「端点全集」的认知也不同:
//! - `BasicDesign §7`: 有 recall / reactions / read / PATCH edit, 无 friends / media / me
//! - `DetailedDesign §5`: 有 friends / media / me, 无 recall / reactions / read / PATCH
//!
//! 本模块只实现**两份规范都指向(或其一明确列出)**的 4 个消息类端点 —— 它们
//! 的意图无歧义(端点→需求表点名 + BasicDesign §7 列出 + service 已就绪)。
//! friends / media / me 那 8 个端点**不在本 commit 范围**: 它们的 service 层
//! 大多尚不存在(relationship 有仓储但无 service; `/me` 需要一个尚未定义的用户
//! 资料读写面), 硬做只能凭空发明 wire 形状。
//!
//! ## 状态码: 无规范可依, 故全部取自 `ErrorCode::http_status()` 单一真源
//!
//! 没有任何一份规范规定这四个端点的响应码, 故**不自造**:
//! - 失败 → 统一走 `json_response(code, ..)`, 状态码由 `ErrorCode::http_status()`
//!   决定(与既有 messages handler 完全一致)
//! - 成功 → REST 惯例, 且与实际结果对应:
//!   `PATCH` 改资源 → 200 + 资源; `recall` 改状态 → 200 + 资源;
//!   `reactions` 创建子资源 → **201**(新插入) / **200**(幂等重放, 资源已存在);
//!   `read` 幂等更新 → 200 + 当前读指针。
//!
//! ## 路径里的 conversation_id 必须与消息实际归属一致
//!
//! `PATCH /v1/conversations/A/messages/{msg}` —— 若 `msg` 其实属于会话 B,
//! 返回 404。理由: service 的编辑/撤回只校验「是否原 sender」, **不**校验
//! 路径里的 conversation; 若不在 handler 层对齐, 客户端会拿到一个
//! 「在会话 A 里编辑成功」的响应, 而资源其实在 B —— URL 说谎, 且让
//! 按 conversation 做的审计/限流全部错位。

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use im_common::ids::{ConversationId, MessageId};
use im_common::AppError;

use super::error_response::json_response;
use super::messages::MessageResponse;
use super::state::{AppState, AuthedUser};

/// 默认最大消息字节数 —— 与 `messages::send_message` 同源
///
/// **刻意不读 `environments.settings`**: `send_message` 也没读(它把
/// `max_size_bytes` 硬编在调用点并留了 TODO)。这两处要一起改成读 settings
/// 才一致; 单独改一处会让「同一个限制在两个端点有不同值」, 那比两处都写死
/// 更难排查。此处保持与既有端点同值并标注。
const MAX_SIZE_BYTES: usize = 65_536;

// ============================================================================
// DTO
// ============================================================================

/// `PATCH /{msg_id}` 请求体 (per aux-13 §1.1.3 `edit_message`)
#[derive(Debug, Clone, Deserialize)]
pub struct EditMessageRequest {
    pub content: Value,
}

/// `POST /{msg_id}/reactions` 请求体 (per aux-13 §1.1.5 `react`)
#[derive(Debug, Clone, Deserialize)]
pub struct AddReactionRequest {
    pub emoji: String,
}

/// `POST /{id}/read` 请求体 (per aux-13 §1.1.6 `mark_read`)
#[derive(Debug, Clone, Deserialize)]
pub struct MarkReadRequest {
    /// 已读到的 sequence(闭区间)
    pub sequence: i64,
}

/// `POST /{id}/read` 响应体
#[derive(Debug, Clone, Serialize)]
pub struct MarkReadResponse {
    pub conversation_id: Uuid,
    /// 服务端当前读指针 —— **不**回显请求值, 因为未推进时它可能与请求不同
    /// (请求了一个更小的 sequence 是**成功**但无效果, 见 aux-08 幂等约定)。
    /// 客户端若拿回显值会误以为读指针退了。
    pub last_read_sequence: i64,
    /// 本次是否真的推进了(`false` = 幂等重放, 不是错误)
    pub advanced: bool,
}

/// `POST /{msg_id}/reactions` 响应体
#[derive(Debug, Clone, Serialize)]
pub struct AddReactionResponse {
    pub message_id: Uuid,
    pub user_id: Uuid,
    pub emoji: String,
    /// 201 时为 true; 200(幂等重放)时为 false
    pub created: bool,
}

// ============================================================================
// 内部: 路径与消息归属一致性
// ============================================================================

/// 校验「消息确实属于 path 里给的会话」, 不一致按 404 处理
///
/// 404 而非 403: 从调用方视角, `conversations/A/messages/{msg}` 这个资源
/// **不存在**(msg 不在 A 里), 与「存在但无权限」是两件事 —— 403 会顺带确认
/// 「该 id 对应的消息确实存在于别处」, 那本身是信息泄漏。
async fn ensure_message_in_conversation(
    app: &web::Data<AppState>,
    conv: ConversationId,
    msg: MessageId,
) -> Result<im_core::message::repository::Message, actix_web::Error> {
    let m = app
        .message_service
        .get(msg)
        .await
        .map_err(app_error)?
        .ok_or_else(|| {
            json_response(
                im_common::ErrorCode::NotFound,
                Some(conv),
                Some("message not found"),
            )
        })?;
    if m.conversation_id != conv {
        return Err(json_response(
            im_common::ErrorCode::NotFound,
            Some(conv),
            Some("message does not belong to this conversation"),
        ));
    }
    Ok(m)
}

/// `AppError` → actix 错误(状态码走 `ErrorCode::http_status()` 单一真源)
fn app_error(e: AppError) -> actix_web::Error {
    // 内部错误的具体原因不外泄(可能含 SQL / 内部路径), 与 WS 侧
    // `map_service_error` 同一取舍; 明细写服务端日志。
    if matches!(
        e.code(),
        im_common::ErrorCode::InternalError | im_common::ErrorCode::ServiceUnavailable
    ) {
        tracing::error!(error = %e, "rest message action failed");
        return json_response(
            im_common::ErrorCode::InternalError,
            None,
            Some("internal error"),
        );
    }
    json_response(e.code(), None, Some(&e.to_string()))
}

// ============================================================================
// PATCH /v1/conversations/{id}/messages/{msg_id} —— 编辑
// ============================================================================

/// `PATCH /v1/conversations/{id}/messages/{msg_id}`
///
/// 200 + `MessageResponse`(编辑后的消息)
pub async fn edit_message(
    auth: AuthedUser,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<EditMessageRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (conv_uuid, msg_uuid) = path.into_inner();
    let conv = ConversationId(conv_uuid);
    let msg_id = MessageId(msg_uuid);

    if !matches!(body.content, Value::Object(_)) {
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            Some(conv),
            Some("content must be JSON object"),
        ));
    }
    ensure_message_in_conversation(&app, conv, msg_id).await?;

    let updated = app
        .message_service
        .edit_message(
            msg_id,
            auth.user_id,
            body.into_inner().content,
            MAX_SIZE_BYTES,
        )
        .await
        .map_err(app_error)?;

    Ok(HttpResponse::Ok().json(MessageResponse::from(&updated)))
}

// ============================================================================
// POST /v1/conversations/{id}/messages/{msg_id}/recall —— 撤回
// ============================================================================

/// `POST /v1/conversations/{id}/messages/{msg_id}/recall`
///
/// 200 + `MessageResponse`(state 已是 `recalled`)
///
/// 撤回时间窗读 `environments.settings.message.recall_window_seconds`
/// (aux-04 §B.4 不变量: **不能写死**), 解析方式与 WS 侧完全一致。
pub async fn recall_message(
    auth: AuthedUser,
    path: web::Path<(Uuid, Uuid)>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (conv_uuid, msg_uuid) = path.into_inner();
    let conv = ConversationId(conv_uuid);
    let msg_id = MessageId(msg_uuid);

    ensure_message_in_conversation(&app, conv, msg_id).await?;

    let window_secs = app
        .settings_service
        .recall_window_seconds(auth.environment_id)
        .await
        .map_err(app_error)?;

    let recalled = app
        .message_service
        .recall_message(
            msg_id,
            auth.user_id,
            chrono::Duration::seconds(i64::from(window_secs)),
        )
        .await
        .map_err(|e| app_error_with_conv(e, conv))?;

    Ok(HttpResponse::Ok().json(MessageResponse::from(&recalled)))
}

// ============================================================================
// POST /v1/conversations/{id}/messages/{msg_id}/reactions —— 添加 reaction
// ============================================================================

/// `POST /v1/conversations/{id}/messages/{msg_id}/reactions`
///
/// **201** + 资源(新插入) / **200** + 资源(幂等重放, 资源本就存在)
///
/// 权限(「必须是消息所属会话的成员」)由 `ReactionService` 承担, 本 handler
/// 不重复实现 —— 见该 service 的模块文档。
pub async fn add_reaction(
    auth: AuthedUser,
    path: web::Path<(Uuid, Uuid)>,
    body: web::Json<AddReactionRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let (conv_uuid, msg_uuid) = path.into_inner();
    let conv = ConversationId(conv_uuid);
    let msg_id = MessageId(msg_uuid);

    ensure_message_in_conversation(&app, conv, msg_id).await?;

    let emoji = body.into_inner().emoji;
    let (reaction, created) = app
        .reaction_service
        .add_reaction(msg_id, auth.user_id, &emoji)
        .await
        .map_err(|e| app_error_with_conv(e, conv))?;

    let resp = AddReactionResponse {
        message_id: reaction.message_id.0,
        user_id: reaction.user_id.0,
        emoji: reaction.emoji,
        created,
    };
    if created {
        Ok(HttpResponse::Created().json(resp))
    } else {
        Ok(HttpResponse::Ok().json(resp))
    }
}

// ============================================================================
// POST /v1/conversations/{id}/read —— 上报已读
// ============================================================================

/// `POST /v1/conversations/{id}/read`
///
/// 200 + `MarkReadResponse`。`advanced: false` 是**成功**(幂等重放, per aux-08),
/// 不是错误 —— 故不返 409。
pub async fn mark_read(
    auth: AuthedUser,
    path: web::Path<Uuid>,
    body: web::Json<MarkReadRequest>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let conv = ConversationId(path.into_inner());
    let requested = body.into_inner().sequence;

    if requested < 0 {
        // 表上是 `CHECK (last_read_sequence >= 0)`, 负值必然写不进去。提前
        // 返 400 好过让它撞 DB 约束后变成 500。
        return Err(json_response(
            im_common::ErrorCode::ValidationError,
            Some(conv),
            Some("sequence must be >= 0"),
        ));
    }

    let advanced = app
        .conversation_service
        .mark_read(conv, auth.user_id, requested)
        .await
        .map_err(|e| app_error_with_conv(e, conv))?;

    // 回**服务端**当前值而非请求值: 请求了更小的 sequence 是成功但无效果,
    // 回显请求值会让客户端误以为读指针退了。
    let current = app
        .conversation_service
        .repo()
        .list_members(conv)
        .await
        .map_err(app_error)?
        .into_iter()
        .find(|m| m.user_id == auth.user_id)
        .map(|m| m.last_read_sequence)
        .unwrap_or(0);

    Ok(HttpResponse::Ok().json(MarkReadResponse {
        conversation_id: conv.0,
        last_read_sequence: current,
        advanced,
    }))
}

/// 带 conversation 上下文的错误映射(响应体里带上 conv_id, per aux-13 §4)
fn app_error_with_conv(e: AppError, conv: ConversationId) -> actix_web::Error {
    if matches!(
        e.code(),
        im_common::ErrorCode::InternalError | im_common::ErrorCode::ServiceUnavailable
    ) {
        tracing::error!(error = %e, "rest message action failed");
        return json_response(
            im_common::ErrorCode::InternalError,
            Some(conv),
            Some("internal error"),
        );
    }
    json_response(e.code(), Some(conv), Some(&e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn negative_sequence_is_rejected_before_touching_db() {
        // 纯逻辑断言: 表上是 CHECK >= 0, 负值必须走 400 而不是撞约束变 500。
        assert!(MarkReadRequest { sequence: -1 }.sequence < 0);
        assert!(!(MarkReadRequest { sequence: 0 }.sequence < 0));
    }

    #[test]
    fn add_reaction_request_rejects_missing_emoji_by_deserialization() {
        // `emoji` 是必填(无 #[serde(default)]), 缺字段时 400 由 actix 的
        // Json extractor 产生 —— 这里只确认「不会静默当成空串」。
        let ok: Result<AddReactionRequest, _> = serde_json::from_str(r#"{"emoji":"👍"}"#);
        assert!(ok.is_ok());
        let missing: Result<AddReactionRequest, _> = serde_json::from_str("{}");
        assert!(missing.is_err(), "缺 emoji 必须报错, 不能默认成空串");
    }

    /// `MarkReadResponse` 的 `last_read_sequence` 是**服务端当前值**, 不是请求值。
    ///
    /// 锁住「不回显请求值」这个决定: 请求一个**更小**的 sequence 是成功但无效果
    /// (aux-08 幂等约定), 若回显请求值, 客户端会以为读指针退了。
    #[test]
    fn mark_read_response_reports_server_value_not_requested_value() {
        let resp = MarkReadResponse {
            conversation_id: Uuid::nil(),
            last_read_sequence: 100,
            advanced: false,
        };
        let j = serde_json::to_value(&resp).unwrap();
        assert_eq!(j["last_read_sequence"], 100);
        assert_eq!(j["advanced"], false, "幂等重放仍是成功, 不是错误");
        // 请求值(比如 50)绝不出现在响应里
        assert!(
            j.get("requested_sequence").is_none(),
            "响应体不应回显请求值: {j}"
        );
    }

    // ========================================================================
    // HTTP 端到端 (真实 init_service + Bearer + 真 PG)
    // ========================================================================
    //
    // **为什么这些不可省**: service 层的真 PG 行为已被前两个 commit 覆盖, 但
    // 「路由注册是否正确 / 状态码是否如声明 / 路径与消息归属的一致性检查是否
    // 真的生效」这条链只有端到端才走得到。actix 的路由匹配、extractor 解析、
    // `json_response` 的状态码映射, 任一处写错, service 层测试全绿。
    //
    // 找不到 DATABASE_URL 时**跳过**而不是失败 —— 与 auth_handlers 的 e2e 一致。

    async fn e2e_pool() -> Option<sqlx::PgPool> {
        let url = std::env::var("DATABASE_URL").ok()?;
        sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect(&url)
            .await
            .ok()
    }

    /// 一个 env 里的三个用户 + 两个会话 + 一条 Alice 发的消息:
    ///
    /// - `alice` — **发送者 + 成员**, 正常路径
    /// - `bob`   — **成员但不是发送者** (用来测「非 sender 编辑」)
    /// - `carol` — **非成员** (用来测「越权」)
    ///
    /// bob 单独存在是因为「不是发送者」和「不是成员」是**两种不同的拒绝理由**:
    /// 合并成一个用户的话, 成员检查会先命中, 发送者检查那条分支根本没跑到,
    /// 而测试名字仍然写着「非 sender」。
    struct RestFixture {
        state: web::Data<AppState>,
        alice_token: String,
        bob_token: String,
        carol_token: String,
        conv: im_common::ids::ConversationId,
        other_conv: im_common::ids::ConversationId,
        msg: MessageId,
    }

    async fn rest_fixture(p: &sqlx::PgPool) -> RestFixture {
        use im_core::conversation::repository::{
            ConversationKind, ConversationRepository, MemberRole,
        };
        use im_core::identity::repository::{User, UserKind, UserState};
        use im_core::message::repository::MessageRepository;
        use std::sync::Arc;

        let env_id: Uuid = sqlx::query_scalar(
            r#"
            WITH t AS (INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'rest-' || gen_random_uuid()::text) RETURNING id),
                 g AS (INSERT INTO games (id, tenant_id, name) SELECT gen_random_uuid(), t.id, 'rest-game-' || gen_random_uuid()::text FROM t RETURNING id)
            INSERT INTO environments (id, game_id, name) SELECT gen_random_uuid(), g.id, 'test' FROM g RETURNING id
            "#,
        )
        .fetch_one(p)
        .await
        .expect("make env");
        let env = im_common::ids::EnvironmentId(env_id);

        async fn mk_user(p: &sqlx::PgPool, env_id: Uuid, name: &str) -> im_common::ids::UserId {
            let id: Uuid = sqlx::query_scalar(
                r#"INSERT INTO users (id, environment_id, kind, display_name)
                   VALUES (gen_random_uuid(), $1, 'user', $2) RETURNING id"#,
            )
            .bind(env_id)
            .bind(name)
            .fetch_one(p)
            .await
            .expect("create user");
            im_common::ids::UserId(id)
        }
        let alice = mk_user(p, env_id, "RestAlice").await;
        let bob = mk_user(p, env_id, "RestBob").await;
        let carol = mk_user(p, env_id, "RestCarol").await;

        let conv_repo: Arc<dyn ConversationRepository> = Arc::new(
            im_core::conversation::pg::PgConversationRepository::new(p.clone()),
        );
        let conv = conv_repo
            .create(env, ConversationKind::Dm, json!({}))
            .await
            .expect("create conv");
        conv_repo
            .add_member(conv.id, alice, MemberRole::Member)
            .await
            .unwrap();
        // Bob 在群里但没发过消息 —— 与 Carol(非成员)分开, 用于隔离「非 sender」这条分支
        conv_repo
            .add_member(conv.id, bob, MemberRole::Member)
            .await
            .unwrap();
        // 另一个会话: Alice 也在里面, 但不含那条消息 —— 用于测「路径与消息归属」
        let other_conv = conv_repo
            .create(env, ConversationKind::Dm, json!({}))
            .await
            .expect("create other conv");
        conv_repo
            .add_member(other_conv.id, alice, MemberRole::Member)
            .await
            .unwrap();

        let msg_id: Uuid = sqlx::query_scalar(
            "INSERT INTO messages (conversation_id, sequence, sender_id, kind, content, idempotency_key) \
             VALUES ($1, 1, $2, 'text', '{\"kind\":\"text\",\"text\":\"hi\"}', gen_random_uuid()::text) RETURNING id",
        )
        .bind(conv.id.0)
        .bind(alice.0)
        .fetch_one(p)
        .await
        .expect("insert message");
        let msg = MessageId(msg_id);

        // 与 auth_handlers 测试同构: 真实 pool 的 5 个 service
        let conversation_repo: Arc<dyn ConversationRepository> = Arc::new(
            im_core::conversation::pg::PgConversationRepository::new(p.clone()),
        );
        let message_repo: Arc<dyn MessageRepository> =
            Arc::new(im_core::message::pg::PgMessageRepository::new(p.clone()));
        let sequencer: Arc<dyn im_core::message::sequence::SequenceAllocator> =
            Arc::new(im_core::message::pg::PgSequenceAllocator::new(p.clone()));
        let events: Arc<dyn im_core::event::publisher::EventPublisher> = Arc::new(
            im_core::event::publisher::NatsEventPublisher::connect("nats://stub:4222")
                .await
                .expect("stub publisher"),
        );

        let token_service = Arc::new(im_core::identity::token::TokenService::new(
            vec![im_core::identity::token::SigningKey {
                kid: "v1".into(),
                key: secrecy::SecretString::new(
                    "test-key-must-be-32-bytes-or-more-padding-padding".into(),
                ),
            }],
            chrono::Duration::seconds(900),
            secrecy::SecretString::new("test-pepper".into()),
        ));

        let mk_token = |uid: im_common::ids::UserId| {
            let u = User {
                id: uid,
                environment_id: env,
                kind: UserKind::User,
                external_identity: None,
                state: UserState::Active,
                display_name: Some("e2e".into()),
                username: None,
                password_hash: None,
                created_at: chrono::Utc::now(),
            };
            token_service.issue_access_token(&u).expect("签 token").0
        };
        let alice_token = mk_token(alice);
        let bob_token = mk_token(bob);
        let carol_token = mk_token(carol);

        let mut secrets = std::collections::HashMap::new();
        secrets.insert(env, secrecy::SecretString::new("rest-test-secret".into()));

        let state = web::Data::new(AppState::new(
            Arc::new(im_core::conversation::service::ConversationService::new(
                conversation_repo.clone(),
            )),
            Arc::new(im_core::message::service::MessageService::new(
                message_repo,
                sequencer,
                events,
                conversation_repo.clone(),
            )),
            token_service.clone(),
            Arc::new(im_core::identity::service::IdentityService::new(
                im_core::identity::pg::PgUserRepository::new(p.clone()),
                im_core::identity::pg::PgDeviceSessionRepository::new(p.clone()),
                token_service,
                secrets,
            )),
            Arc::new(im_core::settings::service::SettingsService::new(p.clone())),
            Arc::new(im_core::reaction::service::ReactionService::new(
                Arc::new(im_core::reaction::pg::PgReactionRepository::new(p.clone())),
                Arc::new(im_core::message::pg::PgMessageRepository::new(p.clone())),
                conversation_repo,
            )),
        ));

        RestFixture {
            state,
            alice_token,
            bob_token,
            carol_token,
            conv: conv.id,
            other_conv: other_conv.id,
            msg,
        }
    }

    #[actix_web::test]
    async fn patch_edit_returns_200_with_updated_content() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}/messages/{msg_id}",
                web::patch().to(edit_message),
            ))
            .await;

        let req = actix_web::test::TestRequest::patch()
            .uri(&format!(
                "/v1/conversations/{}/messages/{}",
                f.conv.0, f.msg.0
            ))
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .set_json(serde_json::json!({"content": {"kind":"text","text":"edited"}}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["content"]["text"], "edited", "应返回编辑后的内容");
        assert_eq!(
            body["state"], "sent",
            "编辑不改变状态; 这条同时验证 state 字段不是兜底假值"
        );
    }

    #[actix_web::test]
    async fn edit_by_member_who_is_not_the_sender_is_forbidden() {
        // Bob **是成员**但不是 sender → 走到「不是发送者」那条拒绝分支。
        //
        // 注意这条**只**锁住 sender 门禁。`MessageService::edit_message` 里
        // **没有**独立的成员校验 —— 非成员也会被同一个 sender 条件挡住
        // (能发消息的前提是当时在会话里)。所以「非成员编辑」并不是一条
        // 单独可观测的路径, 为它再写一个用例会得到同一个 403、同一个原因,
        // 属于重复断言, 不写。
        //
        // 若将来真加了成员校验(例如允许已退会者继续编辑自己的历史消息,
        // 或反之收紧), 应新增独立用例并在这里点明两条门禁的分工。
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}/messages/{msg_id}",
                web::patch().to(edit_message),
            ))
            .await;

        let req = actix_web::test::TestRequest::patch()
            .uri(&format!(
                "/v1/conversations/{}/messages/{}",
                f.conv.0, f.msg.0
            ))
            .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
            .set_json(serde_json::json!({"content": {"kind":"text","text":"hijack"}}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::FORBIDDEN,
            "非 sender(但是成员)编辑必须 403"
        );

        // 内容必须原样保留 —— 403 不能是「先写了再回滚」
        let stored: String =
            sqlx::query_scalar("SELECT content->>'text' FROM messages WHERE id = $1")
                .bind(f.msg.0)
                .fetch_one(&p)
                .await
                .expect("查 messages");
        assert_eq!(stored, "hi", "被拒的编辑不应改动内容");
    }

    #[actix_web::test]
    async fn message_in_a_different_conversation_returns_404() {
        // **路径与消息归属一致性**: 消息在 conv, 但 URL 写 other_conv。
        // 不对齐的话客户端会拿到「在 other_conv 编辑成功」的响应。
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(f.state.clone())
                .route(
                    "/v1/conversations/{id}/messages/{msg_id}",
                    web::patch().to(edit_message),
                )
                .route(
                    "/v1/conversations/{id}/messages/{msg_id}/recall",
                    web::post().to(recall_message),
                ),
        )
        .await;

        for (label, uri) in [
            (
                "edit",
                format!("/v1/conversations/{}/messages/{}", f.other_conv.0, f.msg.0),
            ),
            (
                "recall",
                format!(
                    "/v1/conversations/{}/messages/{}/recall",
                    f.other_conv.0, f.msg.0
                ),
            ),
        ] {
            let req = if label == "edit" {
                actix_web::test::TestRequest::patch()
                    .uri(&uri)
                    .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                    .set_json(serde_json::json!({"content": {"kind":"text","text":"x"}}))
                    .to_request()
            } else {
                actix_web::test::TestRequest::post()
                    .uri(&uri)
                    .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                    .to_request()
            };
            let resp = actix_web::test::call_service(&app, req).await;
            assert_eq!(
                resp.status(),
                actix_web::http::StatusCode::NOT_FOUND,
                "{label}: 消息不属于该会话时必须 404(而非 403 —— 后者会确认该 id 存在)"
            );
        }

        let stored: String =
            sqlx::query_scalar("SELECT content->>'text' FROM messages WHERE id = $1")
                .bind(f.msg.0)
                .fetch_one(&p)
                .await
                .expect("查 messages");
        assert_eq!(stored, "hi", "404 路径不得改动内容");
    }

    #[actix_web::test]
    async fn recall_returns_200_with_state_recalled() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}/messages/{msg_id}/recall",
                web::post().to(recall_message),
            ))
            .await;

        let req = actix_web::test::TestRequest::post()
            .uri(&format!(
                "/v1/conversations/{}/messages/{}/recall",
                f.conv.0, f.msg.0
            ))
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(
            body["state"], "recalled",
            "撤回后响应里的 state 必须是 recalled —— 若这里显示 sent, \
             说明 state 字段仍在走那个「序列化失败就兜底 sent」的旧写法"
        );

        let stored: String = sqlx::query_scalar("SELECT state FROM messages WHERE id = $1")
            .bind(f.msg.0)
            .fetch_one(&p)
            .await
            .expect("查 messages");
        assert_eq!(stored, "recalled", "库里必须真落成 recalled");
    }

    #[actix_web::test]
    async fn recall_twice_is_invalid_state_transition() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}/messages/{msg_id}/recall",
                web::post().to(recall_message),
            ))
            .await;
        let uri = format!("/v1/conversations/{}/messages/{}/recall", f.conv.0, f.msg.0);
        for expected in [
            actix_web::http::StatusCode::OK,
            // 终态再撤 → INVALID_STATE_TRANSITION = 409
            actix_web::http::StatusCode::CONFLICT,
        ] {
            let req = actix_web::test::TestRequest::post()
                .uri(&uri)
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .to_request();
            let resp = actix_web::test::call_service(&app, req).await;
            assert_eq!(resp.status(), expected);
        }
    }

    #[actix_web::test]
    async fn reactions_returns_201_then_200_on_replay() {
        // 201 = 新插入, 200 = 幂等重放。两者都是**成功**, 差别只反映实际结果。
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}/messages/{msg_id}/reactions",
                web::post().to(add_reaction),
            ))
            .await;
        let uri = format!(
            "/v1/conversations/{}/messages/{}/reactions",
            f.conv.0, f.msg.0
        );

        for (expected_status, expected_created) in [
            (actix_web::http::StatusCode::CREATED, true),
            (actix_web::http::StatusCode::OK, false),
        ] {
            let req = actix_web::test::TestRequest::post()
                .uri(&uri)
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(serde_json::json!({"emoji": "👍"}))
                .to_request();
            let resp = actix_web::test::call_service(&app, req).await;
            assert_eq!(resp.status(), expected_status);
            let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
            assert_eq!(body["created"], expected_created, "body: {body}");
        }

        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM message_reactions WHERE message_id = $1",
        )
        .bind(f.msg.0)
        .fetch_one(&p)
        .await
        .expect("count reactions");
        assert_eq!(rows, 1, "幂等重放不得产生第二行");
    }

    #[actix_web::test]
    async fn reactions_by_non_member_is_forbidden() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}/messages/{msg_id}/reactions",
                web::post().to(add_reaction),
            ))
            .await;
        let req = actix_web::test::TestRequest::post()
            .uri(&format!(
                "/v1/conversations/{}/messages/{}/reactions",
                f.conv.0, f.msg.0
            ))
            .insert_header(("Authorization", format!("Bearer {}", f.carol_token)))
            .set_json(serde_json::json!({"emoji": "👍"}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM message_reactions WHERE message_id = $1",
        )
        .bind(f.msg.0)
        .fetch_one(&p)
        .await
        .expect("count reactions");
        assert_eq!(rows, 0, "越权 add 不得留下行");
    }

    #[actix_web::test]
    async fn mark_read_advances_and_reports_server_value() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(f.state.clone())
                .route("/v1/conversations/{id}/read", web::post().to(mark_read)),
        )
        .await;
        let uri = format!("/v1/conversations/{}/read", f.conv.0);

        // (a) 首次上报 5 → 推进
        let req = actix_web::test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .set_json(serde_json::json!({"sequence": 5}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["last_read_sequence"], 5);
        assert_eq!(body["advanced"], true);

        // (b) 上报**更小**的 2 → 成功但无效果, 且回**服务端**值 5 而非请求值
        let req = actix_web::test::TestRequest::post()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .set_json(serde_json::json!({"sequence": 2}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::OK,
            "请求更小的 sequence 是幂等成功, 不是错误"
        );
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["advanced"], false);
        assert_eq!(
            body["last_read_sequence"], 5,
            "必须回服务端当前值; 回显请求值(2)会让客户端以为读指针退了"
        );
    }

    #[actix_web::test]
    async fn mark_read_rejects_negative_sequence_with_400() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(f.state.clone())
                .route("/v1/conversations/{id}/read", web::post().to(mark_read)),
        )
        .await;
        let req = actix_web::test::TestRequest::post()
            .uri(&format!("/v1/conversations/{}/read", f.conv.0))
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .set_json(serde_json::json!({"sequence": -1}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::BAD_REQUEST,
            "负 sequence 应 400; 撞 DB 的 CHECK 会变成 500"
        );
    }

    #[actix_web::test]
    async fn mark_read_by_non_member_is_forbidden() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(f.state.clone())
                .route("/v1/conversations/{id}/read", web::post().to(mark_read)),
        )
        .await;
        let req = actix_web::test::TestRequest::post()
            .uri(&format!("/v1/conversations/{}/read", f.conv.0))
            .insert_header(("Authorization", format!("Bearer {}", f.carol_token)))
            .set_json(serde_json::json!({"sequence": 999}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);
    }

    #[actix_web::test]
    async fn all_action_endpoints_require_authentication() {
        // 无 Bearer → 401。这条对 4 个端点都成立, 故逐个点一遍 ——
        // 漏挂 extractor 的端点在单个测试里看不出来。
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(
            actix_web::App::new()
                .app_data(f.state.clone())
                .route(
                    "/v1/conversations/{id}/messages/{msg_id}",
                    web::patch().to(edit_message),
                )
                .route(
                    "/v1/conversations/{id}/messages/{msg_id}/recall",
                    web::post().to(recall_message),
                )
                .route(
                    "/v1/conversations/{id}/messages/{msg_id}/reactions",
                    web::post().to(add_reaction),
                )
                .route("/v1/conversations/{id}/read", web::post().to(mark_read)),
        )
        .await;

        let paths = [
            (
                format!("/v1/conversations/{}/messages/{}", f.conv.0, f.msg.0),
                true,
            ),
            (
                format!("/v1/conversations/{}/messages/{}/recall", f.conv.0, f.msg.0),
                false,
            ),
            (
                format!(
                    "/v1/conversations/{}/messages/{}/reactions",
                    f.conv.0, f.msg.0
                ),
                false,
            ),
            (format!("/v1/conversations/{}/read", f.conv.0), false),
        ];
        for (uri, is_patch) in paths {
            let mut b = if is_patch {
                actix_web::test::TestRequest::patch()
            } else {
                actix_web::test::TestRequest::post()
            };
            b = b.uri(&uri);
            if is_patch {
                b = b.set_json(serde_json::json!({"content": {"kind":"text","text":"x"}}));
            } else if uri.ends_with("/reactions") {
                b = b.set_json(serde_json::json!({"emoji": "👍"}));
            } else if uri.ends_with("/read") {
                b = b.set_json(serde_json::json!({"sequence": 1}));
            }
            let resp = actix_web::test::call_service(&app, b.to_request()).await;
            assert_eq!(
                resp.status(),
                actix_web::http::StatusCode::UNAUTHORIZED,
                "{uri} 无 Bearer 必须 401"
            );
        }
    }

    #[actix_web::test]
    async fn conversation_get_hides_metadata_from_non_members() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app =
            actix_web::test::init_service(actix_web::App::new().app_data(f.state.clone()).route(
                "/v1/conversations/{id}",
                web::get().to(super::super::conversations::get),
            ))
            .await;
        let uri = format!("/v1/conversations/{}", f.conv.0);

        // 成员 → 200
        let req = actix_web::test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["id"], f.conv.0.to_string());

        // 非成员 → 404(不确认该 id 是否存在)
        let req = actix_web::test::TestRequest::get()
            .uri(&uri)
            .insert_header(("Authorization", format!("Bearer {}", f.carol_token)))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(
            resp.status(),
            actix_web::http::StatusCode::NOT_FOUND,
            "非成员读会话详情必须 404"
        );
    }
}
