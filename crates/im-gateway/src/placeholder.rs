//! 端点占位 — MVP Day 1 后续 commit 替换
//!
//! 所有端点暂时返回 501 Not Implemented,实际 handler 在后续 PR 提交

// 本文件的**唯一职责**就是存放"已定义但尚未接线"的端点桩, 每个函数按定义
// 都不会被调用 —— 这不是"漏了调用",而是文件本身的语义。故此处把原先的
// `dead_code, unused_imports, unused_variables` 三项 blanket 压制收窄到
// `dead_code` 一项: 后两项会连带掩盖本文件未来真实的 import/变量问题。
// 接线完成后应连同本属性一并移除。
#![allow(dead_code)]

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
