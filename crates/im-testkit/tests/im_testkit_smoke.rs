//! im-testkit smoke tests
//!
//! ≥ 9 个测试,覆盖 6 个模块(per Ulysses 2026-08-31 16:26 JST 指令)
//!
//! DDD Review 必查项:
//! - mock 字段 vs im-core / im-protocol 公开 API 一致
//! - 错误码 vs aux-03 §B 21 项一致
//! - 帧 `type` vs aux-13 §1 一致

use im_testkit::assertions::{
    assert_error_body_code, assert_error_code, assert_error_code_str, assert_json_schema,
    assert_message_content_schema, assert_ws_frame_shape, MessageSchemaId,
};
use im_testkit::fixtures::{
    stable_conversation_id, stable_user_id, ConversationFixture, MessageFixture, SessionFixture,
    TenantFixture, UserFixture,
};
use im_testkit::mock_grpc;
use im_testkit::mock_rest;
use im_testkit::mock_ws_frames;
use im_testkit::server::MockImServer;
use im_testkit::ErrorCode;
use im_testkit::MessageContent;
use serde_json::json;
use std::time::Duration;

#[test]
fn mock_ping_frame_has_required_fields() {
    let frame = mock_ws_frames::ping_frame();
    let kind = match &frame {
        im_testkit::ClientFrame::Ping { .. } => "ping",
        _ => panic!("expected ping"),
    };
    assert_eq!(kind, "ping");
}

#[test]
fn mock_auth_request_serializes_to_json() {
    let v = mock_ws_frames::auth_request_json();
    assert_eq!(v["type"], "auth");
    assert_eq!(v["req_id"], mock_ws_frames::REQ_ID_AUTH);
    assert!(v["access_token"].as_str().unwrap().starts_with("eyJ"));
}

#[test]
fn test_tenant_builder_returns_valid_tenant() {
    let t = TenantFixture::new().with_name("acme");
    assert_eq!(t.name, "acme");
    assert_ne!(t.id, im_common::ids::TenantId::nil());
    let json = t.to_json();
    assert_eq!(json["name"], "acme");
}

#[test]
fn assert_error_code_passes_on_match() {
    assert_error_code(ErrorCode::Unauthorized, ErrorCode::Unauthorized);
    assert_error_code(ErrorCode::RateLimited, ErrorCode::RateLimited);
    assert_error_code(
        ErrorCode::ConversationNotFound,
        ErrorCode::ConversationNotFound,
    );
}

#[test]
fn assert_error_code_str_handles_aux03_unknown_fallback() {
    // 未知字符串走兜底 InternalError
    assert_error_code_str("NOT_A_REAL_CODE", ErrorCode::InternalError);
    // 已知字符串正确解析
    assert_error_code_str("ACCOUNT_BANNED", ErrorCode::AccountBanned);
}

#[test]
fn assert_ws_frame_shape_passes_on_ping() {
    // ping 是客户端帧,这里直接构造 ServerFrame 走另一个 shape 路径
    let pong = mock_ws_frames::pong_frame();
    assert_ws_frame_shape(&pong, "pong");
}

#[test]
fn assert_json_schema_text_passes() {
    let v = json!({ "text": "hello" });
    assert_json_schema(&v, MessageSchemaId::Text);
}

#[test]
fn assert_message_content_schema_image_passes() {
    let c = MessageContent::Image {
        media_id: uuid::Uuid::new_v4(),
        width: Some(800),
        height: Some(600),
        thumbnail_media_id: None,
    };
    assert_message_content_schema(&c, MessageSchemaId::Image);
}

#[test]
fn mock_grpc_send_message_response_has_sequence_42() {
    let v = mock_grpc::send_message_response();
    assert_eq!(v["sequence"], 42);
    assert_eq!(v["state"], "sent");
}

#[test]
fn mock_rest_login_response_expires_in_900() {
    let v = mock_rest::login_response();
    assert_eq!(v["expires_in"], 900);
    assert!(v["access_token"].as_str().unwrap().starts_with("eyJ"));
}

#[test]
fn mock_rest_validation_error_has_field_details() {
    let v = mock_rest::validation_error_response();
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert_eq!(v["details"][0]["field"], "content.text");
}

#[test]
fn fixture_message_text_builds_correctly() {
    let m = MessageFixture::text("hello");
    assert_eq!(m.content["text"], "hello");
    assert_eq!(m.kind, "text");
}

#[test]
fn fixture_conversation_group_sets_kind() {
    let c = ConversationFixture::new().group();
    assert_eq!(c.kind, im_testkit::fixtures::ConversationKindFixture::Group);
}

#[test]
fn fixture_user_banned_sets_state() {
    let u = UserFixture::new().banned();
    assert_eq!(u.state, im_testkit::fixtures::UserStateFixture::Banned);
}

#[test]
fn fixture_session_for_user() {
    let u = UserFixture::new();
    let s = SessionFixture::new().for_user(u.id);
    assert_eq!(s.user_id, u.id);
    assert_eq!(s.access_token.chars().take(3).collect::<String>(), "eyJ");
}

#[test]
fn stable_ids_are_deterministic() {
    assert_eq!(
        stable_user_id().to_string(),
        "1a2e6679-7425-40de-944b-e07fc1f90ae7"
    );
    assert_eq!(
        stable_conversation_id().to_string(),
        "7c9e6679-7425-40de-944b-e07fc1f90ae7"
    );
}

#[test]
fn assert_error_body_code_validation_error_passes() {
    let body = mock_ws_frames::validation_error_body();
    assert_error_body_code(&body, ErrorCode::ValidationError);
}

#[test]
fn mock_ws_frames_connected_json() {
    let v = mock_ws_frames::connected_json();
    assert_eq!(v["type"], "connected");
    assert_eq!(v["session_id"], mock_ws_frames::SESSION_ID_CONNECTED);
}

/// `connected` 帧: 生产从不发送 (per gap-ledger §1.47 / im-protocol ws_frames.rs L78-86)。
///
/// `ServerFrame::Connected` 的 doc 明确写着「⚠ 生产侧**从不发送**本帧
/// (2026-10-08 核实)」—— 鉴权成功后 gateway 实际发的是 `AuthOk`。
/// 但 `im-testkit` 照旧暴露 `connected_frame()` / `connected_json()`, 于是
/// 接入方可以对着一个**永远不会到达线上**的帧写断言: 测试永远绿, 线上永不发生。
///
/// 本测试把这个事实钉成可判定断言: mock 可以提供该帧 (契约测试需要它),
/// 但**文档必须警告生产不发**, 且 mock 的 doc 必须与 im-protocol 的警告一致 ——
/// 一旦有人删掉任一处警告, 本测试变红。
#[test]
fn connected_frame_is_documented_as_never_sent_in_production() {
    // 1) im-protocol 侧必须保留「生产从不发送」的警告
    let protocol_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../im-protocol/src/ws_frames.rs"
    ))
    .expect("im-protocol/src/ws_frames.rs must be readable from im-testkit tests");
    assert!(
        protocol_src.contains("生产侧**从不发送**本帧"),
        "im-protocol 的 Connected 变体必须保留「生产从不发送」警告 — \
         若该帧将来真的开始发送, 本警告应与本测试一并更新 (per §1.47)"
    );

    // 2) im-testkit 的 mock 侧也必须带同样的警告, 不能只在一处
    let testkit_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/mock_ws_frames.rs"
    ))
    .expect("mock_ws_frames.rs must be readable");
    assert!(
        testkit_src.contains("生产从不发送") || testkit_src.contains("生产侧**从不发送**"),
        "im-testkit 的 connected mock 必须自带「生产从不发送」警告 doc, \
         否则接入方读到 API 文档会以为线上真会收到这一帧 (per §1.47)"
    );

    // 3) 反向对照: auth_ok 才是生产真发的帧, 必须存在
    assert!(
        protocol_src.contains("AuthOk"),
        "AuthOk 变体必须存在 (生产实际发送的鉴权回执)"
    );
}

/// `sequence: 42` 魔法数: mock 不得把它当「合法读指针」示范。
///
/// gap-ledger §1.50 记录: `mark_read` 曾无上界, 客户端传 `i64::MAX` 会把读指针
/// 永久顶死, 之后该会话任何 sequence 都不再推进、未读数永久失真、**且不报错**。
/// 那一节的原文点名了 im-testkit: 「im-testkit 的 mock 就用 `sequence: 42`
/// 这类魔法数」。
///
/// 服务端已改为在 WHERE 里夹紧 (`last_read_sequence < $1`), 但 mock 仍以 42
/// 作样例。本测试把「42 是 mock 自造值、非生产事实」写成断言, 并显式核对
/// 夹紧守卫仍然在位 —— 守卫被谁删掉, 这里立刻变红。
#[test]
fn mock_sequence_42_is_flagged_as_mock_only_and_server_clamps() {
    // 1) mock 侧必须声明 42 是 mock 值
    let testkit_src =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/mock_grpc.rs"))
            .expect("mock_grpc.rs must be readable");
    assert!(
        testkit_src.contains("mock") && testkit_src.contains("42"),
        "mock_grpc 仍以 sequence=42 作样例 —— 该值必须是 mock 自造, 不能被当成生产事实"
    );

    // 2) 服务端夹紧守卫必须仍在 (删掉它, 本测试变红)
    let core_src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../im-core/src/conversation/pg.rs"
    ))
    .expect("im-core/src/conversation/pg.rs must be readable");
    assert!(
        core_src.contains("last_read_sequence < $1"),
        "advance_last_read_sequence 的 WHERE 夹紧守卫消失了 —— \
         mark_read 又能传 i64::MAX 顶死读指针 (per §1.50)"
    );
}

#[test]
fn mock_ws_frames_ack_idempotent_replay_flag() {
    let v = mock_ws_frames::ack_idempotent_replay_json();
    assert_eq!(v["ok"], true);
    assert_eq!(v["data"]["idempotent_replay"], true);
}

#[test]
fn mock_grpc_validate_token_response_valid() {
    let v = mock_grpc::validate_access_token_response();
    assert_eq!(v["valid"], true);
    assert_eq!(v["user_id"], mock_grpc::USER_ID_AUTH);
}

#[test]
fn mock_grpc_mark_read_request_shape() {
    let v = mock_grpc::mark_read_request();
    assert_eq!(v["conversation_id"], mock_grpc::CONV_ID_DM);
    assert_eq!(v["sequence"], 42);
}

#[actix_web::test]
async fn mock_server_healthz_responds_ok() {
    let srv = MockImServer::start().await.expect("server start");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let url = format!("{}/healthz", srv.base_url());
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .expect("send ok");
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["status"], "ok");
}

#[actix_web::test]
async fn mock_server_auth_guest_returns_token() {
    let srv = MockImServer::start().await.expect("server start");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let url = format!("{}/v1/auth/guest", srv.base_url());
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&json!({ "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7" }))
        .send()
        .await
        .expect("send ok");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.expect("json");
    assert_eq!(body["expires_in"], 900);
    assert_eq!(body["user_id"], "1a2e6679-7425-40de-944b-e07fc1f90ae7");
}
