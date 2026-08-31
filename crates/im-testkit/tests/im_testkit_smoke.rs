//! im-testkit smoke tests
//!
//! ≥ 9 个测试,覆盖 6 个模块(per Ulysses 2026-08-31 16:26 JST 指令)
//!
//! DDD Review 必查项:
//! - mock 字段 vs im-core / im-protocol 公开 API 一致
//! - 错误码 vs aux-03 §B 21 项一致
//! - 帧 `type` vs aux-13 §1 一致

use im_testkit::assertions::{
    assert_error_body_code, assert_error_code, assert_error_code_str,
    assert_json_schema, assert_message_content_schema, assert_ws_frame_shape,
    MessageSchemaId,
};
use im_testkit::fixtures::{
    stable_conversation_id, stable_user_id, ConversationFixture, MessageFixture,
    SessionFixture, TenantFixture, UserFixture,
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
    assert_error_code(ErrorCode::ConversationNotFound, ErrorCode::ConversationNotFound);
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
