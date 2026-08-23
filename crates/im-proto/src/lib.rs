//! im-proto: 共享 protobuf 定义 + tonic-build 生成代码
//!
//! 内部 gRPC 契约(im-gateway ⇄ im-core): package `im.core.v1`
//!
//! 禁止任何服务手写与生成代码重复的结构体(避免契约漂移)

#![allow(clippy::all)]

// 由 build.rs 从 proto/core.proto 生成
pub mod im {
    pub mod core {
        pub mod v1 {
            tonic::include_proto!("im.core.v1");
        }
    }
}
