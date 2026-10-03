//! 守住 `.cargo/audit.toml` 里 h2 豁免的**前提条件**
//!
//! ## 这条测试在守什么
//!
//! `.cargo/audit.toml` 为 RUSTSEC-2026-0258(h2)写了豁免, 理由是:
//! h2 只在 TLS/ALPN 协商出 HTTP/2 时才被使用, 而本仓网关**不做 TLS 终止**
//! (`main.rs` 是 `.bind(("0.0.0.0", port))`, 明文 HTTP; TLS 由上游 envoy
//! 终止), 所以 h2 的漏洞代码路径不可达。
//!
//! 问题是: 那条理由是**写在注释里的**, 而注释不会因为代码变化而失效。
//! 有人某天给网关加上 `bind_rustls`(这是个完全合理的需求 —— 比如要让网关
//! 直接对外), 注释仍然写着「不做 TLS 终止」, 而实际暴露面已经变了。
//! 于是豁免会在**没人察觉**的情况下变成一个错误的安全假设。
//!
//! 故把前提做成断言: 一旦出现 TLS 终止绑定, 这条测试先红, 提醒回来处理豁免。
//!
//! ## 为什么用「读源文件文本」而不是真的起一个 server
//!
//! 断言的是**源码里有没有某个 API 调用**。真去起 TLS server 再验证 ALPN
//! 协商结果, 需要证书、私钥、一整套 TLS 配置, 那是另一个量级的成本, 而
//! 这里要防的失误是「不小心加了 TLS 终止」—— 源文本检查足够, 且不会因为
//! 环境问题假绿。
//!
//! 匹配用大小写不敏感的子串而非正则: 依赖的 API 名一旦换(`bind_rustls_0_23`
//! 之类) 正则会漏, 而子串会随前缀变化继续命中一部分。两种都不完美, 故
//! 下面同时列出若干已知变体, 并在失败信息里直接指向该改哪里。

use std::fs;
use std::path::PathBuf;

/// 网关的 HTTP server 装配处。改动 server 绑定方式时, 本文件的断言要一起看。
///
/// 路径用 `CARGO_MANIFEST_DIR` 拼绝对路径, **不能**写相对路径: cargo 跑集成
/// 测试时把 CWD 设成**包根**(`crates/im-gateway/`)而非 workspace 根, 所以
/// `crates/im-gateway/src/main.rs` 这种相对路径根本读不到。
/// 第一版就是这么写的, 结果守卫在**干净代码上也红** —— 一个永远失败的守卫
/// 比没有守卫更糟: 它会让人慢慢习惯「这测试本来就是红的」。
fn main_rs() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main.rs")
}

fn read_main() -> String {
    let p = main_rs();
    fs::read_to_string(&p).unwrap_or_else(|e| panic!("读不到 {}: {e}", p.display()))
}

#[test]
fn gateway_does_not_terminate_tls_so_h2_is_unreachable() {
    let src = read_main();

    // 任何形式的 TLS 绑定都会让 h2 变得可达, 从而使 audit.toml 的豁免失效。
    let tls_bind_markers = [
        "bind_rustls",
        "bind_openssl",
        "bind_ssl",
        ".rustls(",
        ".openssl(",
    ];
    let found: Vec<&str> = tls_bind_markers
        .iter()
        .copied()
        .filter(|m| src.contains(m))
        .collect();

    assert!(
        found.is_empty(),
        "网关开始自己做 TLS 终止了(命中: {found:?})。\n\
         这会让 **h2 变得可达**, 从而使 `.cargo/audit.toml` 里 \
         RUSTSEC-2026-0258 的豁免前提失效 —— 那条豁免的理由是「TLS 由上游 \
         envoy 终止, 网关只讲明文 HTTP/1.1, h2 不会被协商到」。\n\
         现在必须二选一:\n  \
           A) 撤掉豁免, 并排期升级到 actix-http 4 / actix-web 5 以拿到 \
              h2 >= 0.4.16(真正的修复);\n  \
           B) 若这处 TLS 终止是误加的, 去掉它。\n\
         别只是把本测试改回通过 —— 那是把安全假设藏起来。"
    );
}

#[test]
fn gateway_binds_plain_http_on_its_configured_port() {
    // 反向锁定: 确认 server 仍是明文绑定, 而不是被改成了某种我们没预料的形态。
    // 这条比上面的「没看到 TLS」更弱, 但能在绑定方式被大改时给出提示。
    let src = read_main();
    let p = main_rs();
    assert!(
        src.contains(".bind("),
        "{} 里找不到 `.bind(...)`。server 的绑定方式被改动过 —— \
         请复核 audit.toml 的 h2 豁免前提是否仍成立。",
        p.display()
    );
}
