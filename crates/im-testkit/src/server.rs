//! Mock IM Server —— 最小版,供 im-gateway 集成测试用
//!
//! 起 actix-web 监听 `127.0.0.1:0`(随机端口),响应:
//! - `GET /healthz` → 200 `{"status":"ok"}`
//! - `GET /v1/auth/guest` → 200 mock token pair
//! - `POST /v1/auth/token/exchange` → 200 mock token pair
//! - `POST /v1/conversations` → 201 mock conversation
//! - `POST /v1/conversations/{id}/messages` → 200 mock message
//! - `GET  /v1/conversations/{id}/messages` → 200 mock list
//! - `POST /v1/media/presign` → 200 mock presign
//! - `WS  /ws` → 回响 `pong` / `connected` / `ack` 帧(收到 `ping`/`auth`/`send_message`)
//!
//! **不模拟 im-gateway 完整业务**(不发异步广播、不持久化);**仅作为 wire 协议对端**,
//! 让 im-gateway 集成测试能跑通 HTTP/WS 请求-响应。

use std::io;

use actix_web::{
    middleware::Logger, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use serde_json::{json, Value};

/// Mock IM Server handle
#[derive(Clone, Debug)]
pub struct MockImServer {
    /// 已绑定的 host:port(如 `127.0.0.1:54321`)
    pub addr: String,
}

impl MockImServer {
    /// 起一个 mock server,绑定 `127.0.0.1:0`(随机端口)
    ///
    /// 调用方负责持有 `MockImServer` 直到测试结束;当 handle 析构时,后台 server 任务继续
    /// 跑(actix-web `HttpServer` 行为),测试结束进程退出时回收。
    pub async fn start() -> io::Result<Self> {
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let server = HttpServer::new(move || {
            App::new()
                .wrap(Logger::default())
                .route("/healthz", web::get().to(healthz))
                .route("/readyz", web::get().to(readyz))
                .service(
                    web::scope("/v1")
                        .route("/auth/guest", web::post().to(auth_guest))
                        .route("/auth/token/exchange", web::post().to(auth_token_exchange))
                        .route("/auth/refresh", web::post().to(auth_refresh))
                        .route(
                            "/conversations",
                            web::post().to(conversations_create),
                        )
                        .route(
                            "/conversations/{id}/messages",
                            web::post().to(messages_send),
                        )
                        .route(
                            "/conversations/{id}/messages",
                            web::get().to(messages_list),
                        )
                        .route("/media/presign", web::post().to(media_presign)),
                )
                .route("/ws", web::get().to(ws_echo))
        })
        .listen(listener)?
        .run();

        // 在后台 spawn server;不 await
        tokio::spawn(server);

        Ok(MockImServer {
            addr: addr.to_string(),
        })
    }

    /// 拼出 base URL(如 `http://127.0.0.1:54321`)
    pub fn base_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// 拼出 WS URL(如 `ws://127.0.0.1:54321/ws`)
    pub fn ws_url(&self) -> String {
        format!("ws://{}/ws", self.addr)
    }
}

// =============================================================================
// HTTP handlers
// =============================================================================

async fn healthz() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

async fn readyz() -> impl Responder {
    HttpResponse::Ok().json(json!({"status": "ready"}))
}

async fn auth_guest(_req: HttpRequest, _body: web::Json<Value>) -> impl Responder {
    HttpResponse::Ok().json(json!({
        "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
        "refresh_token": "rt_9b8e6679-7425-40de-944b-e07fc1f90ae7",
        "user_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
        "expires_in": 900,
    }))
}

async fn auth_token_exchange(_req: HttpRequest, _body: web::Json<Value>) -> impl Responder {
    HttpResponse::Ok().json(json!({
        "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
        "refresh_token": "rt_9b8e6679-7425-40de-944b-e07fc1f90ae7",
        "user_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
        "expires_in": 900,
    }))
}

async fn auth_refresh(_req: HttpRequest, _body: web::Json<Value>) -> impl Responder {
    HttpResponse::Ok().json(json!({
        "access_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
        "refresh_token": "rt_refreshed_9b8e6679",
        "user_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
        "expires_in": 900,
    }))
}

async fn conversations_create(_req: HttpRequest, _body: web::Json<Value>) -> impl Responder {
    HttpResponse::Created().json(json!({
        "id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        "kind": "dm",
        "metadata": {},
        "created_at": "2026-08-23T00:00:00Z",
    }))
}

async fn messages_send(
    _req: HttpRequest,
    _body: web::Json<Value>,
) -> impl Responder {
    HttpResponse::Ok().json(json!({
        "id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
        "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
        "sequence": 42,
        "sender_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
        "kind": "text",
        "content": { "text": "echo from mock" },
        "reply_to": null,
        "state": "sent",
        "created_at": "2026-08-23T00:00:00Z",
        "edited_at": null,
        "reactions": []
    }))
}

async fn messages_list(_req: HttpRequest) -> impl Responder {
    HttpResponse::Ok().json(json!({
        "messages": [
            {
                "id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
                "conversation_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7",
                "sequence": 42,
                "sender_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7",
                "kind": "text",
                "content": { "text": "hi" },
                "reply_to": null,
                "state": "sent",
                "created_at": "2026-08-23T00:00:00Z",
                "edited_at": null,
                "reactions": []
            }
        ],
        "next_cursor": "43",
        "has_more": false,
    }))
}

async fn media_presign(_req: HttpRequest, _body: web::Json<Value>) -> impl Responder {
    HttpResponse::Ok().json(json!({
        "upload_url": "https://mock-minio.example.com/upload?X-Amz-Mock=1",
        "media_id": "5c6e6679-7425-40de-944b-e07fc1f90ae7",
        "expires_at": "2026-08-23T01:00:00Z",
    }))
}

// =============================================================================
// WebSocket echo —— 收到帧按 type 回写固定响应
// =============================================================================

async fn ws_echo(req: HttpRequest, stream: web::Payload) -> actix_web::Result<HttpResponse> {
    let (response, mut session, msg_stream) = actix_ws::handle(&req, stream)?;

    actix_web::rt::spawn(async move {
        use futures::StreamExt;
        let mut msg_stream = msg_stream;

        // 第一帧:`connected`
        let _ = session
            .text(
                serde_json::to_string(&json!({
                    "type": "connected",
                    "session_id": "9b8e6679-7425-40de-944b-e07fc1f90ae7"
                }))
                .unwrap(),
            )
            .await;

        while let Some(Ok(msg)) = msg_stream.next().await {
            match msg {
                actix_ws::Message::Text(text) => {
                    let parsed: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let ty = parsed.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    let reply = match ty {
                        "ping" => json!({ "type": "pong", "ts": parsed.get("ts").cloned().unwrap_or(json!(0)) }),
                        "auth" => json!({
                            "type": "ack",
                            "req_id": parsed.get("req_id").cloned().unwrap_or(json!(null)),
                            "ok": true,
                            "data": { "user_id": "1a2e6679-7425-40de-944b-e07fc1f90ae7" }
                        }),
                        "send_message" => json!({
                            "type": "ack",
                            "req_id": parsed.get("req_id").cloned().unwrap_or(json!(null)),
                            "ok": true,
                            "data": {
                                "message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae7",
                                "sequence": 42
                            }
                        }),
                        _ => json!({ "type": "ack", "ok": false, "error": { "code": "INTERNAL_ERROR" } }),
                    };
                    let _ = session.text(serde_json::to_string(&reply).unwrap()).await;
                }
                actix_ws::Message::Ping(bytes) => {
                    let _ = session.pong(&bytes).await;
                }
                actix_ws::Message::Close(reason) => {
                    let _ = session.close(reason).await;
                    break;
                }
                _ => {}
            }
        }
    });

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[actix_web::test]
    async fn mock_server_starts_and_healthz_responds() {
        let srv = MockImServer::start().await.expect("start ok");
        // 给后台 task 一小段启动时间
        tokio::time::sleep(Duration::from_millis(100)).await;
        let url = format!("{}/healthz", srv.base_url());
        let resp = reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(5))
            .send()
            .await
            .expect("send ok");
        assert!(resp.status().is_success());
        let body: Value = resp.json().await.expect("json");
        assert_eq!(body["status"], "ok");
    }

    #[actix_web::test]
    async fn mock_server_auth_guest_returns_token_pair() {
        let srv = MockImServer::start().await.expect("start ok");
        tokio::time::sleep(Duration::from_millis(100)).await;
        let url = format!("{}/v1/auth/guest", srv.base_url());
        let resp = reqwest::Client::new()
            .post(&url)
            .json(&json!({ "environment_id": "7c9e6679-7425-40de-944b-e07fc1f90ae7" }))
            .send()
            .await
            .expect("send ok");
        assert_eq!(resp.status(), 200);
        let body: Value = resp.json().await.expect("json");
        assert_eq!(body["expires_in"], 900);
        assert!(body["access_token"].as_str().unwrap().starts_with("eyJ"));
    }

    #[actix_web::test]
    async fn mock_server_base_url_format() {
        let srv = MockImServer::start().await.expect("start ok");
        assert!(srv.base_url().starts_with("http://127.0.0.1:"));
        assert!(srv.ws_url().starts_with("ws://127.0.0.1:"));
        assert!(srv.ws_url().ends_with("/ws"));
    }
}
