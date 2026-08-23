//! tonic-build: 从 proto/ 生成 Rust 代码
//!
//! 依据: ImplementationSpec §2.1 crates/im-proto/ 布局

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/core.proto"], &["proto"])?;
    Ok(())
}
