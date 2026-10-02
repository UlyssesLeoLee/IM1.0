//! im-proto: 共享 protobuf 定义 + tonic-build 生成代码
//!
//! 内部 gRPC 契约(im-gateway ⇄ im-core): package `im.core.v1`
//!
//! 禁止任何服务手写与生成代码重复的结构体(避免契约漂移)

#![allow(clippy::all)] // 全仓唯一的 blanket allow,保留理由见下:
                       // 本 crate 唯一的手写代码就是下面这层 `include_proto!` 转发, 实质内容全部由
                       // tonic-build 生成 (OUT_DIR/im.core.v1.rs)。该生成代码会稳定触发
                       // `result_large_err` 等 lint (2026-10-03 实测 22 处), 而生成物不受我们控制。
                       // 待办: 收窄到 `#[allow(...)] pub mod generated { include_proto!(...) }`,
                       //       使手写部分重新受 clippy 管辖。

// 由 build.rs 从 proto/core.proto 生成
pub mod im {
    pub mod core {
        pub mod v1 {
            tonic::include_proto!("im.core.v1");
        }
    }
}
