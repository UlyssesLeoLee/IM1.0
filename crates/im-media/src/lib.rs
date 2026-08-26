//! im-media 占位 — V1+ 实装
//!
//! MVP 阶段: 仅暴露空 trait,workspace 完整
//! V1 阶段: 接入 aws-sdk-s3 生成 MinIO 预签名 URL、上传、下载
//!
//! 依据: ImplementationSpec §1.1 / §3.1.5

#![allow(dead_code, unused_imports, unused_variables)] // 2026-08-26 Day 2 GATE: 占位模块,clippy -D warnings 通过;V1 实装时移除
use async_trait::async_trait;
use uuid::Uuid;

pub struct PresignResult {
    pub upload_url: String,
    pub media_id: Uuid,
    pub expires_at_unix: i64,
}

#[async_trait]
pub trait MediaService: Send + Sync {
    async fn presign_upload(
        &self,
        user_id: Uuid,
        content_type: &str,
        size_hint: i64,
    ) -> Result<PresignResult, im_common::AppError>;

    async fn presign_download(
        &self,
        media_id: Uuid,
    ) -> Result<String, im_common::AppError>;
}

pub fn placeholder() {}
