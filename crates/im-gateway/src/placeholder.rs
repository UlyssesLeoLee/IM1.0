//! 端点占位 — MVP Day 1 后续 commit 替换
//!
//! 所有端点暂时返回 501 Not Implemented,实际 handler 在后续 PR 提交

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use actix_web::HttpResponse;
use serde_json::json;

pub async fn healthz() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ok"}))
}

pub async fn readyz() -> HttpResponse {
    HttpResponse::Ok().json(json!({"status": "ready"}))
}

pub async fn metrics() -> HttpResponse {
    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body("# MVP: prometheus not yet enabled\n")
}

pub async fn token_exchange() -> HttpResponse {
    HttpResponse::NotImplemented().json(json!({
        "code": "INTERNAL_ERROR",
        "message": "endpoint placeholder, see ImplementationSpec §3.1.1"
    }))
}

pub async fn guest() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn refresh() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn link() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn logout() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn conv_create() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn conv_list() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn conv_get() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn msg_list() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}

pub async fn members_list() -> HttpResponse {
    HttpResponse::NotImplemented().finish()
}
