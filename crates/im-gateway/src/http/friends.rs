//! 好友关系端点 —— `POST /v1/friends/requests` / `.../{id}/respond` / `.../{id}/block`
//!
//! 依据: DetailedDesign §5 端点表 + aux-11 §4 时序图 + aux-13 §3.7 样例
//!
//! ## 三个端点的形状都有规范出处, 无一字段是本实现自拟
//!
//! | 端点 | body | 成功响应 | 出处 |
//! |---|---|---|---|
//! | `POST /v1/friends/requests` | `{recipient_id}` | **204 No Content** | aux-11 §4 (line 300 / 329) |
//! | `POST /v1/friends/requests/{id}/respond` | `{accept}` | **204 No Content** | aux-13 §3.7 (`[PROTOCOL-FROZEN-PATCH]`) |
//! | `POST /v1/friends/{id}/block` | 无 | **204 No Content** | aux-08 (line 349) |
//!
//! ## 为什么成功一律 204 而不是 201
//!
//! aux-11 line 328-329 明确写着「已是好友 → `Empty` / 204 No Content」, 而
//! proto 里三个 RPC 全部返回 `google.protobuf.Empty`。既然「已是好友」和
//! 「新建申请」两条路径**返回同一个东西**, 就不能给后者返 201 + body ——
//! 那会让「幂等重发」和「首次创建」在响应上无法区分, 而这两条路径规范明确
//! 要求同样对待。
//!
//! aux-11 line 327 有一句「返 200 + 现有 friendship」的注释, 但它紧跟的
//! line 328 是 `Core-->>Auth: Empty`、line 329 是 `204 No Content`。
//! **以可执行的时序图主线为准**: 注释描述的是早期设想, 图与 proto 一致。
//! 此处按 204 实装并记录该矛盾于 `docs/gap-ledger.md` §1.16。
//!
//! ## 业务规则全部在 `RelationshipService` 内
//!
//! 三个 handler 只做「解析 → 调 service → 映射状态码」, **不重写任何规则**:
//! - 自己给自己发申请 / 拉黑自己 → `Validation`
//! - 被对方拉黑 → `UserBlocked` (403)
//! - 重复申请 → `FRIEND_REQUEST_EXISTS` (409)
//! - 非 recipient 响应他人申请 → `Forbidden` (403)
//! - 已处理过的申请重复响应 → `INVALID_STATE_TRANSITION` (409)
//!
//! 若 handler 层另写一遍, 与 WS 侧必然漂移, 漂移的那侧就是越权漏洞。

use actix_web::web;
use actix_web::HttpResponse;
use serde::Deserialize;
use uuid::Uuid;

use im_common::ids::UserId;
use im_common::AppError;

use super::error_response::json_response;
use super::state::{AppState, AuthedUser};

/// `POST /v1/friends/requests` 的 body
///
/// aux-11 §4 line 300: `Body: {recipient_id}`。`environment_id` 与 `sender_id`
/// **不来自 body** —— 前者取自 access token 的 claims, 后者是调用者自己,
/// 两者都不可由客户端指定(否则可以冒充他人发申请)。
#[derive(Deserialize)]
pub struct SendFriendRequestBody {
    pub recipient_id: Uuid,
}

/// `POST /v1/friends/requests/{id}/respond` 的 body
///
/// aux-13 §3.7: `{ "accept": true }` / `{ "accept": false }`。
/// `accept` 必填 —— 无 `#[serde(default)]`, 缺字段时由 actix 的 `Json`
/// extractor 产 400, 而不是静默当成「拒绝」。
#[derive(Deserialize)]
pub struct RespondFriendRequestBody {
    pub accept: bool,
}

/// `AppError` → HTTP 错误 (状态码由 `ErrorCode::http_status()` 单一真源决定)
///
/// 与 `message_actions::app_error` 同构, 但**不带** `conversation_id` 上下文
/// (好友关系不属于任何会话)。
fn app_error(e: AppError) -> actix_web::Error {
    if matches!(
        e.code(),
        im_common::ErrorCode::InternalError | im_common::ErrorCode::ServiceUnavailable
    ) {
        // 内部错误不外泄具体原因(可能含 SQL / 内部路径); 明细写服务端日志。
        tracing::error!(error = %e, "friends endpoint failed");
        return json_response(
            im_common::ErrorCode::InternalError,
            None,
            Some("internal error"),
        );
    }
    json_response(e.code(), None, Some(&e.to_string()))
}

/// `POST /v1/friends/requests` —— 发送好友申请
///
/// **204 No Content** (aux-11 §4 line 329)。以下路径同返 204:
/// - 新建 pending 申请
/// - 已是好友(无需申请)
pub async fn send_request(
    auth: AuthedUser,
    body: web::Json<SendFriendRequestBody>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let recipient = UserId(body.into_inner().recipient_id);
    app.relationship_service
        .send_request(auth.environment_id, auth.user_id, recipient)
        .await
        .map_err(app_error)?;
    Ok(HttpResponse::NoContent().finish())
}

/// `POST /v1/friends/requests/{id}/respond` —— 接受 / 拒绝
///
/// **204 No Content**, 接受与拒绝**均无 body** (aux-13 §3.7 line 718-722)。
/// 接受时服务端在同一事务内 UPDATE `friend_requests` + INSERT 两条对偶
/// `friendships` 记录(aux-13 line 724-727), 该事务在仓储层。
pub async fn respond_request(
    auth: AuthedUser,
    path: web::Path<Uuid>,
    body: web::Json<RespondFriendRequestBody>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let request_id = path.into_inner();
    app.relationship_service
        .respond_request(request_id, auth.user_id, body.into_inner().accept)
        .await
        .map_err(app_error)?;
    Ok(HttpResponse::NoContent().finish())
}

/// `POST /v1/friends/{id}/block` —— 拉黑
///
/// **204 No Content** (aux-08 line 349)。重复拉黑幂等 —— 仓储层是
/// `INSERT ... ON CONFLICT DO UPDATE SET state='blocked'`, 第二次调用同样
/// 成功(aux-08 line 227 明确把它列为幂等重试可安全重放的例子)。
pub async fn block(
    auth: AuthedUser,
    path: web::Path<Uuid>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let target = UserId(path.into_inner());
    app.relationship_service
        .block(auth.user_id, target)
        .await
        .map_err(app_error)?;
    Ok(HttpResponse::NoContent().finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_support::{e2e_pool, rest_fixture};
    use serde_json::json;

    /// 返回**已配好路由的 App**, 由各测试自己 `init_service(..).await`。
    ///
    /// 不返回已初始化的 service: 那个类型要写 `actix_http::Request` 作泛型
    /// 参数, 而 `actix_http` 不是本 crate 的直接依赖(只有传递依赖)。
    fn friends_app(
        f: &crate::http::test_support::RestFixture,
    ) -> actix_web::App<
        impl actix_web::dev::ServiceFactory<
            actix_web::dev::ServiceRequest,
            Config = (),
            Response = actix_web::dev::ServiceResponse,
            Error = actix_web::Error,
            InitError = (),
        >,
    > {
        actix_web::App::new()
            .app_data(f.state.clone())
            .route("/v1/friends/requests", web::post().to(send_request))
            .route(
                "/v1/friends/requests/{id}/respond",
                web::post().to(respond_request),
            )
            .route("/v1/friends/{id}/block", web::post().to(block))
    }

    /// 三个端点成功时**都**是 204 且**都**无 body —— 这正是「幂等重发与首次
    /// 创建无法区分」的代价与好处。
    #[actix_web::test]
    async fn send_request_returns_204_with_no_body() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        let req = actix_web::test::TestRequest::post()
            .uri("/v1/friends/requests")
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .set_json(json!({"recipient_id": f.bob.0}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);
        assert!(
            actix_web::test::read_body(resp).await.is_empty(),
            "204 不应带 body(aux-11 line 329 / aux-13 line 718)"
        );

        // 库里确实建了 pending 申请
        let state: String = sqlx::query_scalar(
            "SELECT state FROM friend_requests WHERE sender_id = $1 AND recipient_id = $2",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("查 friend_requests");
        assert_eq!(state, "pending");
    }

    /// 重复申请同一对用户 → 409 FRIEND_REQUEST_EXISTS, 而**不是** 204。
    ///
    /// 这条是「204 幂等」的反面: 幂等的是**已存在**的申请, 但仓库里
    /// `UNIQUE(env, sender, recipient)` 跨 state 阻断, 第二次调用是冲突。
    /// aux-11 line 333-338 明确要求 409。
    #[actix_web::test]
    async fn duplicate_request_returns_409_friend_request_exists() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        for expected in [
            actix_web::http::StatusCode::NO_CONTENT,
            actix_web::http::StatusCode::CONFLICT,
        ] {
            let req = actix_web::test::TestRequest::post()
                .uri("/v1/friends/requests")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(json!({"recipient_id": f.bob.0}))
                .to_request();
            let resp = actix_web::test::call_service(&app, req).await;
            assert_eq!(resp.status(), expected);
            if expected == actix_web::http::StatusCode::CONFLICT {
                let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
                assert_eq!(body["code"], "FRIEND_REQUEST_EXISTS", "body: {body}");
            }
        }
    }

    /// 接受申请 → 204, 且**事务内**建出两条对偶 friendships 记录。
    ///
    /// 「两条」是 aux-13 line 724-725 的硬要求 (A→B 与 B→A 各一条),
    /// 只建一条的话 `list_friends` 对双方就不一致。
    #[actix_web::test]
    async fn accept_creates_two_symmetric_friendship_rows() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        let req = actix_web::test::TestRequest::post()
            .uri("/v1/friends/requests")
            .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
            .set_json(json!({"recipient_id": f.bob.0}))
            .to_request();
        assert_eq!(
            actix_web::test::call_service(&app, req).await.status(),
            actix_web::http::StatusCode::NO_CONTENT
        );

        let request_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM friend_requests WHERE sender_id = $1 AND recipient_id = $2",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("取 request_id");

        let req = actix_web::test::TestRequest::post()
            .uri(&format!("/v1/friends/requests/{request_id}/respond"))
            .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
            .set_json(json!({"accept": true}))
            .to_request();
        let resp = actix_web::test::call_service(&app, req).await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);

        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM friendships \
             WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("count friendships");
        assert_eq!(rows, 2, "接受后必须有 A→B 与 B→A 两条对偶记录");
    }

    /// 拒绝 → 204, 且**不**插 friendships。
    #[actix_web::test]
    async fn reject_marks_request_rejected_and_creates_no_friendship() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri("/v1/friends/requests")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(json!({"recipient_id": f.bob.0}))
                .to_request(),
        )
        .await;

        let request_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM friend_requests WHERE sender_id = $1 AND recipient_id = $2",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("取 request_id");

        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri(&format!("/v1/friends/requests/{request_id}/respond"))
                .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
                .set_json(json!({"accept": false}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);

        let state: String = sqlx::query_scalar("SELECT state FROM friend_requests WHERE id = $1")
            .bind(request_id)
            .fetch_one(&p)
            .await
            .expect("查 state");
        assert_eq!(state, "rejected");

        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*)::bigint FROM friendships \
             WHERE (user_id = $1 AND friend_id = $2) OR (user_id = $2 AND friend_id = $1)",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("count friendships");
        assert_eq!(rows, 0, "拒绝时不得插入 friendships(aux-13 line 727)");
    }

    /// **非 recipient** 响应他人申请 → 403, 且 state 必须仍是 pending。
    ///
    /// 权限校验必须**先于**状态更新: 若先 UPDATE 再校验, 被拒的调用已经把
    /// 申请消费掉了, 而调用方拿到的是 403 —— 状态被一个失败请求改掉了。
    #[actix_web::test]
    async fn respond_by_non_recipient_is_forbidden_and_leaves_state_pending() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri("/v1/friends/requests")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(json!({"recipient_id": f.bob.0}))
                .to_request(),
        )
        .await;
        let request_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM friend_requests WHERE sender_id = $1 AND recipient_id = $2",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("取 request_id");

        // Carol 既不是 sender 也不是 recipient
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri(&format!("/v1/friends/requests/{request_id}/respond"))
                .insert_header(("Authorization", format!("Bearer {}", f.carol_token)))
                .set_json(json!({"accept": true}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        let state: String = sqlx::query_scalar("SELECT state FROM friend_requests WHERE id = $1")
            .bind(request_id)
            .fetch_one(&p)
            .await
            .expect("查 state");
        assert_eq!(
            state, "pending",
            "403 的请求不得消费掉这条申请(校验必须先于写入)"
        );
    }

    /// 已处理过的申请重复响应 → 409 (aux-13 §3.7 line 756)。
    #[actix_web::test]
    async fn responding_twice_returns_409() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri("/v1/friends/requests")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(json!({"recipient_id": f.bob.0}))
                .to_request(),
        )
        .await;
        let request_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM friend_requests WHERE sender_id = $1 AND recipient_id = $2",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("取 request_id");

        for expected in [
            actix_web::http::StatusCode::NO_CONTENT,
            actix_web::http::StatusCode::CONFLICT,
        ] {
            let resp = actix_web::test::call_service(
                &app,
                actix_web::test::TestRequest::post()
                    .uri(&format!("/v1/friends/requests/{request_id}/respond"))
                    .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
                    .set_json(json!({"accept": true}))
                    .to_request(),
            )
            .await;
            assert_eq!(resp.status(), expected);
        }
    }

    /// 拉黑 → 204, 且落成 `friendships.state='blocked'`。
    #[actix_web::test]
    async fn block_marks_friendship_blocked() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri(&format!("/v1/friends/{}/block", f.bob.0))
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::NO_CONTENT);

        let state: String = sqlx::query_scalar(
            "SELECT state FROM friendships WHERE user_id = $1 AND friend_id = $2",
        )
        .bind(f.alice.0)
        .bind(f.bob.0)
        .fetch_one(&p)
        .await
        .expect("查 friendships");
        assert_eq!(state, "blocked");
    }

    /// 拉黑自己 → 400 (`Validation`)。不是 403: 这是请求本身不合法。
    #[actix_web::test]
    async fn block_self_returns_400() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri(&format!("/v1/friends/{}/block", f.alice.0))
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }

    /// 已拉黑对方后, 对方发来的申请必须被拒 (403 USER_BLOCKED)。
    #[actix_web::test]
    async fn blocked_user_cannot_send_friend_request() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;

        // Bob 先拉黑 Alice
        assert_eq!(
            actix_web::test::call_service(
                &app,
                actix_web::test::TestRequest::post()
                    .uri(&format!("/v1/friends/{}/block", f.alice.0))
                    .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
                    .to_request(),
            )
            .await
            .status(),
            actix_web::http::StatusCode::NO_CONTENT
        );

        // Alice 反过来向 Bob 发申请 → 403
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri("/v1/friends/requests")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(json!({"recipient_id": f.bob.0}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::FORBIDDEN);

        let rows: i64 =
            sqlx::query_scalar("SELECT count(*)::bigint FROM friend_requests WHERE sender_id = $1")
                .bind(f.alice.0)
                .fetch_one(&p)
                .await
                .expect("count");
        assert_eq!(rows, 0, "被拉黑方的申请不得落库");
    }

    /// 三个端点无 Bearer 一律 401 (漏挂 extractor 的端点测不出来)。
    #[actix_web::test]
    async fn all_friend_endpoints_require_authentication() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;
        let some_id = "7c9e6679-7425-40de-944b-e07fc1f90ae7";
        for uri in [
            "/v1/friends/requests".to_string(),
            format!("/v1/friends/requests/{some_id}/respond"),
            format!("/v1/friends/{some_id}/block"),
        ] {
            let resp = actix_web::test::call_service(
                &app,
                actix_web::test::TestRequest::post()
                    .uri(&uri)
                    .set_json(json!({"recipient_id": f.bob.0, "accept": true}))
                    .to_request(),
            )
            .await;
            assert_eq!(
                resp.status(),
                actix_web::http::StatusCode::UNAUTHORIZED,
                "{uri} 无 Bearer 必须 401"
            );
        }
    }

    /// `accept` 必填: 缺字段应 400, 不能静默当成「拒绝」。
    #[actix_web::test]
    async fn respond_without_accept_field_is_400() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(friends_app(&f)).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::post()
                .uri("/v1/friends/requests/7c9e6679-7425-40de-944b-e07fc1f90ae7/respond")
                .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
                .set_json(json!({}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::BAD_REQUEST);
    }
}
