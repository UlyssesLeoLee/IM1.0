//! 用户资料端点 —— `GET /v1/me` / `PATCH /v1/me`
//!
//! 依据: proto `GetMeRequest` / `UpdateMeRequest` + `CoreService.GetMe/UpdateMe`
//!       (均返回 `User` message), DetailedDesign §5 端点表
//!
//! ## 为什么必须有独立的 `MeResponse` 而不能直接返回 `im_core::User`
//!
//! `im_core::identity::repository::User` **派生了 `Serialize`**, 且带有:
//! ```rust
//! pub username: Option<String>,
//! pub password_hash: Option<String>,   // argon2id PHC-format
//! ```
//! handler 里一句 `HttpResponse::Ok().json(user)` 就会把 **argon2id 密码哈希**
//! 发到客户端。这不是「多返回了一个字段」, 是凭据泄漏。
//!
//! 根因是 `User` 承担了**两个角色**: 数据库行 与 对外表示。前者需要全部列,
//! 后者只应暴露 proto `User` message 里的那 7 个字段。`MeResponse` 就是把
//! 这两个角色拆开的地方。
//!
//! ## 字段集的依据
//!
//! `aux-13` **没有** `/v1/me` 的 REST 样例(它在 §3 只给了
//! `media/presign` 和 `friends/requests/{id}/respond`)。故字段集取自 proto
//! `User` message —— 那是规范自己给出的「用户对外表示」的定义:
//!
//! | proto `User` 字段 | `MeResponse` |
//! |---|---|
//! | `id` | `id` |
//! | `environment_id` | `environment_id` |
//! | `kind` (user/guest) | `kind` |
//! | `external_identity_json` | `external_identity`(结构化对象, 见下) |
//! | `state` (active/banned/...) | `state` |
//! | `display_name` (optional) | `display_name` |
//! | `created_at` | `created_at` |
//!
//! **一处刻意的偏离**: proto 里外部身份是 `string external_identity_json`
//! —— 因为 protobuf 当时没引 `google.protobuf.Value`(注释里写了)。REST 侧
//! 没有这个约束, 双层编码(JSON 字符串套 JSON)对客户端纯属负担, 故按
//! `ExternalIdentity { provider, external_uid }` 结构化返回。
//!
//! ## 刻意**不**返回的字段
//!
//! | 字段 | 原因 |
//! |---|---|
//! | `password_hash` | argon2id 哈希, 绝对不能出服务端 |
//! | `username` | 登录凭证; proto `User` 也**没有**这个字段 —— 规范本身就把它排除在对外表示之外 |
//!
//! 注意本端点是 `/me`, 返回的是**调用者自己**的 `external_identity`, 不构成
//! 越权(与 `GET /v1/friends` 不同 —— 那里返回的是**他人**资料, 那才是泄漏;
//! 该端点因 `Friend` vs `User` 规范矛盾未实装, 见 gap-ledger §1.16)。

use actix_web::web;
use actix_web::HttpResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use im_common::ids::{EnvironmentId, UserId};
use im_core::identity::repository::{ExternalIdentity, User, UserKind, UserState};

use super::error_response::json_response;
use super::state::{AppState, AuthedUser};

/// `GET` / `PATCH /v1/me` 的响应体
///
/// 字段集见模块文档。`From<&User>` 是**唯一**的构造入口 —— 让「忘记剔除
/// 敏感字段」这件事变成编译期可见的代码审查点, 而不是每个 handler 各写一遍
/// 字段列表。
#[derive(Serialize)]
pub struct MeResponse {
    pub id: Uuid,
    pub environment_id: Uuid,
    pub kind: UserKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_identity: Option<ExternalIdentity>,
    pub state: UserState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<&User> for MeResponse {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.0,
            environment_id: u.environment_id.0,
            kind: u.kind,
            external_identity: u.external_identity.clone(),
            state: u.state,
            display_name: u.display_name.clone(),
            created_at: u.created_at,
        }
    }
}

/// `PATCH /v1/me` 的 body
///
/// proto `UpdateMeRequest { user_id, display_name? }`:
/// - `user_id` **不来自 body** —— 取自 access token claims。客户端若能指定
///   它, 就能改别人的资料。
/// - `display_name` 可缺省; 缺省 = 本次不改动(语义见
///   `IdentityService::update_me` 的文档, 那里记了「显式 null 清空」这个
///   尚未裁决的分支)。
#[derive(Deserialize)]
pub struct UpdateMeBody {
    #[serde(default)]
    pub display_name: Option<String>,
}

fn app_error(e: im_common::AppError) -> actix_web::Error {
    if matches!(
        e.code(),
        im_common::ErrorCode::InternalError | im_common::ErrorCode::ServiceUnavailable
    ) {
        tracing::error!(error = %e, "me endpoint failed");
        return json_response(
            im_common::ErrorCode::InternalError,
            None,
            Some("internal error"),
        );
    }
    json_response(e.code(), None, Some(&e.to_string()))
}

/// `GET /v1/me` —— 当前用户资料
pub async fn get_me(
    auth: AuthedUser,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let user = app
        .identity_service
        .get_me(auth.user_id)
        .await
        .map_err(app_error)?;
    Ok(HttpResponse::Ok().json(MeResponse::from(&user)))
}

/// `PATCH /v1/me` —— 更新资料, 返回**更新后**的用户
///
/// 返回更新后的实体(而非 204)是有依据的: proto `UpdateMe` 返回 `User`,
/// 客户端一次往返即可拿到新值, 不必再 GET 一次。
pub async fn update_me(
    auth: AuthedUser,
    body: web::Json<UpdateMeBody>,
    app: web::Data<AppState>,
) -> Result<HttpResponse, actix_web::Error> {
    let display_name = body.into_inner().display_name;
    let user = app
        .identity_service
        .update_me(auth.user_id, display_name.as_deref())
        .await
        .map_err(app_error)?;
    Ok(HttpResponse::Ok().json(MeResponse::from(&user)))
}

/// 编译期护栏: 确认 newtype 与 uuid 转换在两个 handler 里都成立。
#[allow(dead_code)]
fn _type_anchors(_: UserId, _: EnvironmentId) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::test_support::{e2e_pool, rest_fixture};
    use serde_json::json;

    /// 返回**已配好路由的 App**, 由各测试自己 `init_service(..).await`。
    ///
    /// 不返回已初始化的 service: 那个类型要写 `actix_http::Request` 作泛型
    /// 参数, 而 `actix_http` 不是本 crate 的直接依赖(只有传递依赖)。
    fn me_app(
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
            .route("/v1/me", web::get().to(get_me))
            .route("/v1/me", web::patch().to(update_me))
    }

    /// **本文件最重要的一条**: 响应体绝不能出现 `password_hash`。
    ///
    /// `im_core::User` 派生了 `Serialize` 且带 argon2id 哈希, 所以任何一句
    /// `.json(user)` 都会把凭据发出去。这条断言就是那个护栏 —— 若有人后来
    /// 「图省事」把 `MeResponse` 换成直接序列化 `User`, 立刻变红。
    #[actix_web::test]
    async fn get_me_never_exposes_password_hash_or_username() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        // 先给 Alice 装上真正的密码哈希, 确认不是「因为没值所以看不见」
        sqlx::query("UPDATE users SET username = $2, password_hash = $3 WHERE id = $1")
            .bind(f.alice.0)
            .bind("alice_login")
            .bind("$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$aGFzaGhoZXNo")
            .execute(&p)
            .await
            .expect("写密码哈希");

        let app = actix_web::test::init_service(me_app(&f)).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::get()
                .uri("/v1/me")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        let raw = actix_web::test::read_body(resp).await;
        let text = String::from_utf8(raw.to_vec()).expect("utf8");
        assert!(
            !text.contains("argon2id"),
            "响应里出现了 argon2id 哈希: {text}"
        );
        assert!(
            !text.contains("alice_login"),
            "响应里出现了登录名 username: {text}"
        );

        let body: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(body["id"], f.alice.0.to_string());
        assert_eq!(body["kind"], "user");
        assert!(body.get("password_hash").is_none());
        assert!(body.get("username").is_none());
    }

    /// 只能读到**自己**: Bob 的 token 拿不到 Alice 的资料。
    #[actix_web::test]
    async fn get_me_returns_the_caller_not_an_arbitrary_user() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        sqlx::query("UPDATE users SET display_name = 'AliceReal' WHERE id = $1")
            .bind(f.alice.0)
            .execute(&p)
            .await
            .expect("改名");
        sqlx::query("UPDATE users SET display_name = 'BobReal' WHERE id = $1")
            .bind(f.bob.0)
            .execute(&p)
            .await
            .expect("改名");

        let app = actix_web::test::init_service(me_app(&f)).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::get()
                .uri("/v1/me")
                .insert_header(("Authorization", format!("Bearer {}", f.bob_token)))
                .to_request(),
        )
        .await;
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["id"], f.bob.0.to_string(), "必须返回调用者自己");
        assert_eq!(body["display_name"], "BobReal");
    }

    /// PATCH 改 display_name 并返回**更新后**的实体 (proto `UpdateMe` 返回 `User`)。
    #[actix_web::test]
    async fn patch_me_updates_display_name_and_returns_new_value() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(me_app(&f)).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::patch()
                .uri("/v1/me")
                .insert_header(("Authorization", format!("Bearer {}", f.alice_token)))
                .set_json(json!({"display_name": "新名字"}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);
        let body: serde_json::Value = actix_web::test::read_body_json(resp).await;
        assert_eq!(body["display_name"], "新名字", "响应须是更新后的值");

        // 库里也确实改了
        let stored: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
                .bind(f.alice.0)
                .fetch_one(&p)
                .await
                .expect("查 users");
        assert_eq!(stored.as_deref(), Some("新名字"));
    }

    /// PATCH 只能改自己 —— body 里没有 user_id 字段可指定他人。
    ///
    /// 这条同时锁住「`UpdateMeBody` 不含 `user_id`」这个设计决定: 一旦有人
    /// 为了「方便」加上 `user_id`, serde 会开始接受它, 越权就打开了。
    #[actix_web::test]
    async fn patch_me_body_cannot_target_another_user() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(me_app(&f)).await;
        let resp = actix_web::test::call_service(
            &app,
            actix_web::test::TestRequest::patch()
                .uri("/v1/me")
                .insert_header(("Authorization", format!("Bearer {}", f.carol_token)))
                .set_json(json!({"display_name": "被改", "user_id": f.alice.0}))
                .to_request(),
        )
        .await;
        assert_eq!(resp.status(), actix_web::http::StatusCode::OK);

        // Alice 的名字必须没被动过
        let stored: Option<String> =
            sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
                .bind(f.alice.0)
                .fetch_one(&p)
                .await
                .expect("查 users");
        assert_ne!(
            stored.as_deref(),
            Some("被改"),
            "body 里的 user_id 绝不能被采信(字段根本不该存在)"
        );
    }

    /// 无 Bearer → 401 (GET 与 PATCH 两个方法都要点)。
    #[actix_web::test]
    async fn me_endpoints_require_authentication() {
        let Some(p) = e2e_pool().await else { return };
        let f = rest_fixture(&p).await;
        let app = actix_web::test::init_service(me_app(&f)).await;
        for (method, has_body) in [
            (actix_web::http::Method::GET, false),
            (actix_web::http::Method::PATCH, true),
        ] {
            let mut b = actix_web::test::TestRequest::default()
                .method(method.clone())
                .uri("/v1/me");
            if has_body {
                b = b.set_json(json!({"display_name": "x"}));
            }
            let resp = actix_web::test::call_service(&app, b.to_request()).await;
            assert_eq!(
                resp.status(),
                actix_web::http::StatusCode::UNAUTHORIZED,
                "{method} /v1/me 无 Bearer 必须 401"
            );
        }
    }

    /// 纯逻辑: `MeResponse` 的字段集是**白名单**, 不是「User 减去几个字段」。
    ///
    /// 锁住「加字段必须显式改 `From` 实现」这件事: 若有人用 `#[serde(flatten)]`
    /// 或直接序列化 `User`, 这里的断言会先失效。
    #[test]
    fn me_response_field_set_is_an_explicit_allowlist() {
        let u = User {
            id: UserId(Uuid::new_v4()),
            environment_id: EnvironmentId(Uuid::new_v4()),
            kind: UserKind::Guest,
            external_identity: Some(ExternalIdentity {
                provider: "steam".into(),
                external_uid: "u-1".into(),
            }),
            state: UserState::Active,
            display_name: Some("n".into()),
            username: Some("secret_login".into()),
            password_hash: Some("$argon2id$hash".into()),
            created_at: Utc::now(),
        };
        let v = serde_json::to_value(MeResponse::from(&u)).expect("serialize");
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|s| s.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "created_at",
                "display_name",
                "environment_id",
                "external_identity",
                "id",
                "kind",
                "state"
            ],
            "字段集必须与 proto User message 一致, 且不含 password_hash / username"
        );
    }
}
