//! WS **端到端**测试 —— 2026-10-03, 全仓第一个
//!
//! ## 为什么是这个文件而不是加个依赖
//!
//! 仓库里没有任何 WS 客户端依赖(`tokio-tungstenite` / `awc` 之类), 而本机
//! Docker 与 Git 都配了走 `127.0.0.1:7897` 的代理、**该代理进程没在运行**,
//! crates.io 不可达 —— 于是「加个测试依赖」这条路此刻是断的。
//!
//! 与其等, 不如写一个**只实现用得到的部分**的客户端: HTTP upgrade 握手 +
//! 帧编解码约 200 行。没实现的分片重组、压缩、扩展协商, 在这些用例里根本
//! 不会出现; 唯一必须实现完整的是 **126 / 127 两种扩展长度** —— 「第一条
//! 消息就超过 125 字节」是随手就能撞上的, 只解短帧的客户端会给出**假绿灯**。
//!
//! 掩码键用固定值: RFC 要求客户端掩码是为了防缓存投毒攻击中间层, 而这里是
//! loopback 上没有代理的测试, 随机化不产生任何额外覆盖。
//!
//! ## 这些用例抓到过什么
//!
//! 写下第一个用例(`ws_is_reachable_at_the_documented_path`)时它就**是红的**:
//! `main.rs` 把 `http::configure` 挂在 `/v1` 下, `http/mod.rs` 又给它套了一层
//! `scope("/ws")`, 而 `ws::router::configure` 内部**已经自带** `scope("/ws")`
//! —— 实际路径是 `/v1/ws/ws`, 而 `ws/router.rs` 自己的文档写的是 `/v1/ws`。
//! 也就是说**按规范接的客户端根本连不上**, 且这件事此前无人发现: 因为
//! 全仓没有任何测试会真的发起一次 WS 连接。

use std::net::SocketAddr;
use std::time::Duration;

use actix_web::web;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use super::hub::WsHub;
use crate::http::test_support::{e2e_pool, rest_fixture, RestFixture};

// ============================================================================
// 最小 WS 客户端
// ============================================================================

/// `Sec-WebSocket-Key` —— 取 RFC 6455 §1.3 的示例值, base64 解码后恰好 16 字节。
const WS_KEY: &str = "dGhlIHNhbXBsZSBub25jZQ==";

#[derive(Debug, PartialEq)]
enum Event {
    Text(String),
    Binary(Vec<u8>),
    /// 协议层 Pong(opcode 0xA)。服务端用**文本帧**回业务 pong, 所以测试里
    /// 看到的 pong 走 `Text`; 这个变体只在服务端发协议层 pong 时出现。
    Pong(Vec<u8>),
    Close,
}

enum Out {
    Text(String),
    Pong(Vec<u8>),
}

pub struct TestWs {
    out: mpsc::UnboundedSender<Out>,
    events: mpsc::UnboundedReceiver<Event>,
}

/// 带前缀缓冲的读半边
///
/// 握手响应读到的字节里可能已经夹着 WS 帧(服务端若在 upgrade 后立刻推帧),
/// 这部分不能丢。`pending` + `pos` 避免每读一个字节就 `drain` 一次。
struct FrameReader {
    inner: tokio::net::tcp::OwnedReadHalf,
    pending: Vec<u8>,
    pos: usize,
}

impl FrameReader {
    /// 确保 `pending` 里还剩至少 `n` 个未消费字节
    async fn fill(&mut self, n: usize) -> Result<(), String> {
        while self.pending.len() - self.pos < n {
            let mut tmp = [0u8; 4096];
            let k = self
                .inner
                .read(&mut tmp)
                .await
                .map_err(|e| format!("read: {e}"))?;
            if k == 0 {
                return Err("连接已关闭".into());
            }
            self.pending.extend_from_slice(&tmp[..k]);
        }
        Ok(())
    }

    fn take(&mut self, n: usize) -> Vec<u8> {
        let v = self.pending[self.pos..self.pos + n].to_vec();
        self.pos += n;
        if self.pos >= 65_536 {
            self.pending.drain(..self.pos);
            self.pos = 0;
        }
        v
    }

    /// 读一帧; `Ok(None)` = 对端正常关闭
    async fn read_frame(&mut self) -> Result<Option<(bool, u8, Vec<u8>)>, String> {
        let head = match self.fill(2).await {
            Ok(()) => self.take(2),
            Err(e) if e == "连接已关闭" => return Ok(None),
            Err(e) => return Err(e),
        };
        let fin = head[0] & 0x80 != 0;
        let opcode = head[0] & 0x0F;
        let masked = head[1] & 0x80 != 0;
        let len7 = (head[1] & 0x7F) as usize;

        let len = match len7 {
            126 => {
                self.fill(2).await?;
                u16::from_be_bytes(self.take(2).try_into().unwrap()) as usize
            }
            127 => {
                self.fill(8).await?;
                u64::from_be_bytes(self.take(8).try_into().unwrap()) as usize
            }
            n => n,
        };

        let key = if masked {
            self.fill(4).await?;
            Some(self.take(4))
        } else {
            None
        };

        self.fill(len).await?;
        let mut payload = self.take(len);
        // 服务端→客户端按 RFC 必须**不**掩码, 但解掩是无害的: 万一掩码位
        // 被置上, 至少不会读出乱码再报一句莫名其妙的 JSON 解析错。
        if let Some(k) = key {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= k[i % 4];
            }
        }
        Ok(Some((fin, opcode, payload)))
    }
}

/// 客户端→服务端的帧**必须**带掩码位, 否则服务端按 RFC 应当拒绝连接。
fn build_frame(opcode: u8, payload: &[u8]) -> Vec<u8> {
    const MASK: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d];
    let n = payload.len();
    let mut v = Vec::with_capacity(n + 14);
    v.push(0x80 | opcode);
    if n < 126 {
        v.push(0x80 | n as u8);
    } else if n <= u16::MAX as usize {
        v.push(0x80 | 126);
        v.extend_from_slice(&(n as u16).to_be_bytes());
    } else {
        v.push(0x80 | 127);
        v.extend_from_slice(&(n as u64).to_be_bytes());
    }
    v.extend_from_slice(&MASK);
    v.extend(payload.iter().enumerate().map(|(i, b)| b ^ MASK[i % 4]));
    v
}

impl TestWs {
    pub async fn connect(addr: SocketAddr, path: &str) -> Result<Self, String> {
        let mut stream = TcpStream::connect(addr)
            .await
            .map_err(|e| format!("connect {addr}: {e}"))?;
        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {addr}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {WS_KEY}\r\n\
             Sec-WebSocket-Version: 13\r\n\r\n"
        );
        stream
            .write_all(req.as_bytes())
            .await
            .map_err(|e| format!("write upgrade: {e}"))?;

        // 读响应头直到空行
        let mut buf: Vec<u8> = Vec::new();
        let mut tmp = [0u8; 1024];
        let (leftover, head_len) = loop {
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| format!("read upgrade: {e}"))?;
            if n == 0 {
                return Err(format!(
                    "upgrade 无响应。状态行: {}",
                    String::from_utf8_lossy(&buf[..buf.len().min(120)])
                ));
            }
            buf.extend_from_slice(&tmp[..n]);
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break (buf[p + 4..].to_vec(), p + 4);
            }
            if buf.len() > 16_384 {
                return Err("upgrade 响应头过大".into());
            }
        };

        let head = String::from_utf8_lossy(&buf[..head_len]).to_lowercase();
        if !head.starts_with("http/1.1 101") {
            return Err(format!(
                "upgrade 被拒(状态行: {:?})。**看返回里有没有 `ws`**: \
                 若规范路径 404 而某个带额外 `/ws` 的路径能连, 说明存在重复 scope",
                head.lines().next().unwrap_or("")
            ));
        }
        // 必须看到 `Sec-WebSocket-Accept` 才算真的完成了 WS 握手。
        //
        // 只看 101 是不够的: 101 是「协议升级被接受」, 而这个头才是
        // 「服务端确实按 RFC 6455 做了握手」的证据。缺它时后续所有收发断言
        // 都在测一个不确定是否存在的连接。
        if !head.contains("sec-websocket-accept:") {
            return Err(format!("101 响应里没有 Sec-WebSocket-Accept: {head}"));
        }

        let (read_half, write_half) = stream.into_split();
        let mut reader = FrameReader {
            inner: read_half,
            pending: leftover,
            pos: 0,
        };

        let (otx, mut orx) = mpsc::unbounded_channel::<Out>();
        let (etx, erx) = mpsc::unbounded_channel::<Event>();

        // 写协程
        tokio::spawn(async move {
            let mut w = write_half;
            while let Some(o) = orx.recv().await {
                let frame = match o {
                    Out::Text(s) => build_frame(0x1, s.as_bytes()),
                    Out::Pong(p) => build_frame(0xA, &p),
                };
                if w.write_all(&frame).await.is_err() {
                    break;
                }
            }
        });

        // 读协程: 顺带应答协议层 ping
        let pong_tx = otx.clone();
        tokio::spawn(async move {
            let mut cont = Vec::new();
            loop {
                match reader.read_frame().await {
                    Ok(None) => break,
                    Ok(Some((fin, opcode, payload))) => match opcode {
                        0x0 => {
                            cont.extend_from_slice(&payload);
                            if fin {
                                let _ = etx
                                    .send(Event::Text(String::from_utf8_lossy(&cont).into_owned()));
                                cont.clear();
                            }
                        }
                        0x1 if fin => {
                            let _ = etx
                                .send(Event::Text(String::from_utf8_lossy(&payload).into_owned()));
                        }
                        0x1 => cont.extend_from_slice(&payload),
                        0x2 => {
                            let _ = etx.send(Event::Binary(payload));
                        }
                        0x8 => break,
                        0x9 => {
                            let _ = pong_tx.send(Out::Pong(payload));
                        }
                        0xA => {
                            let _ = etx.send(Event::Pong(payload));
                        }
                        _ => {}
                    },
                    Err(_) => break,
                }
            }
            let _ = etx.send(Event::Close);
        });

        Ok(Self {
            out: otx,
            events: erx,
        })
    }

    pub fn send_text(&self, s: &str) -> Result<(), String> {
        self.out
            .send(Out::Text(s.to_string()))
            .map_err(|_| "写协程已退出".to_string())
    }

    /// 取下一个**文本**帧; 连接关闭时报错
    pub async fn next_text(&mut self, within: Duration) -> Result<String, String> {
        let fut = async {
            loop {
                match self.events.recv().await {
                    Some(Event::Text(s)) => return Ok(s),
                    Some(Event::Binary(b)) => {
                        return Err(format!("收到二进制帧 {} 字节, 期望文本帧", b.len()))
                    }
                    Some(Event::Pong(_)) => continue,
                    Some(Event::Close) | None => return Err("连接被服务端关闭".to_string()),
                }
            }
        };
        tokio::time::timeout(within, fut)
            .await
            .map_err(|_| format!("{} 内没有收到文本帧", within.as_millis()))?
    }

    pub async fn next_json(&mut self, within: Duration) -> Result<serde_json::Value, String> {
        let s = self.next_text(within).await?;
        serde_json::from_str(&s).map_err(|e| format!("不是合法 JSON: {e}; 原文: {s}"))
    }

    /// 断言 `within` 内**没有**任何帧到达
    pub async fn expect_silence(&mut self, within: Duration) -> Result<(), String> {
        let got = tokio::time::timeout(within, self.events.recv()).await;
        match got {
            Err(_) => Ok(()),
            Ok(None) => Ok(()),
            Ok(Some(e)) => Err(format!("本该静默, 却收到了 {e:?}")),
        }
    }
}

// ============================================================================
// 装配: 与 main.rs 同构(同样的 /v1 scope + 同样的 app_data)
// ============================================================================

struct Harness {
    addr: SocketAddr,
}

async fn harness() -> Option<(Harness, RestFixture)> {
    let p = e2e_pool().await?;
    let f = rest_fixture(&p).await;
    let state = f.state.clone();
    let hub = web::Data::new(WsHub::new());
    // 生产是 main.rs: `App::new().app_data(state).app_data(hub).service(scope("/v1")...)`
    // —— 这里照抄同一段, 不给测试另开一条捷径路由。
    let factory = move || {
        actix_web::App::new()
            .app_data(state.clone())
            .app_data(hub.clone())
            .service(web::scope("/v1").configure(crate::http::configure))
    };

    // actix-web 4.13 **没有** `test::start`(已从 3.x 移除), 所以自己起。
    // 绑端口 0 让内核分配空闲端口, 再回读实际地址 —— 写死端口会在并行
    // 跑测试时互相抢占, 且失败信息是「Address already in use」, 与真正
    // 的原因(端口号写死)毫无关联。
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    listener.set_nonblocking(true).ok()?;
    let addr = listener.local_addr().ok()?;
    let server = actix_web::HttpServer::new(factory)
        .listen(listener)
        .ok()?
        .run();
    // spawn 而非 await: await 会把测试挂死在 server 的永动流上。
    actix_web::rt::spawn(server);
    Some((Harness { addr }, f))
}

macro_rules! hz {
    () => {
        match harness().await {
            Some(x) => x,
            None => {
                // 静默跳过是本仓最大的假绿灯向量(台账 §1.15), 而**本文件开头
                // 就栽在它上面过**: Docker 停掉后 6 个用例全部「通过」, 实际
                // 一个都没跑 —— 因为「跳过」在 test harness 里就是「通过」。
                // 统一走共享实现, 免得两处同义逻辑各自漂移。
                crate::http::test_support::skip_or_fail_pg("ws::e2e (需要 PG)");
                return;
            }
        }
    };
}

/// 辅助: 连上 + 鉴权, 返回「auth_ok 之前」收不到东西这件事已被断言过的客户端
async fn authed(h: &Harness, token: &str) -> Result<TestWs, String> {
    let mut c = TestWs::connect(h.addr, "/v1/ws").await?;
    c.send_text(&serde_json::json!({"type":"auth","access_token":token}).to_string())?;
    let ack = c.next_json(Duration::from_secs(5)).await?;
    if ack.get("type").and_then(|t| t.as_str()) != Some("auth_ok") {
        return Err(format!("鉴权未通过, 收到: {ack}"));
    }
    Ok(c)
}

// ============================================================================
// 用例
// ============================================================================

/// 规范路径 `/v1/ws` 必须真的挂在路由表上 —— **不依赖 PG**
///
/// 2026-10-03 发现: `main.rs` 把 `http::configure` 挂在 `/v1` 下, `http/mod.rs`
/// 又给它套了一层 `scope("/ws")`, 而 `ws::router::configure` 内部**已经自带**
/// `scope("/ws")` —— actix 的 scope 是**嵌套**的, 于是实际路径成了
/// `/v1/ws/ws`。按规范接的客户端根本连不上, 且全仓没有任何测试会真的发起
/// 一次 WS 连接, 所以此前无人发现。
///
/// ## 为什么这条不连 PG
///
/// 「路由挂在哪」这个问题跟数据库毫无关系。原先这条用例走完整夹具(要 PG),
/// 于是 Docker 一停它就**静默跳过并报通过** —— 「跳过」在 test harness 里
/// 就是「通过」, 于是缺陷看起来是「有测试且绿的」。
///
/// 判据用「非 404」而不是「101」: 不注入 `web::Data<AppState>` 时, 路由若
/// 命中, `ws_handler` 会在提取 `web::Data<AppState>` 时失败返 500; 路由若
/// 没命中则返 404。**500 与 404 恰好把「路由在不在」和「handler 能不能跑」
/// 这两件事分开**, 于是这条断言完全不需要能跑起来的依赖。
#[actix_web::test]
async fn ws_route_is_registered_at_the_documented_path() {
    let app = actix_web::test::init_service(
        actix_web::App::new().service(web::scope("/v1").configure(crate::http::configure)),
    )
    .await;

    for path in ["/v1/ws", "/v1/ws/"] {
        let req = actix_web::test::TestRequest::get().uri(path).to_request();
        let status = actix_web::test::call_service(&app, req).await.status();
        assert_ne!(
            status.as_u16(),
            404,
            "{path} 没有注册到路由表。ws/router.rs 的文档声称是 /v1/ws, \
             而 http/mod.rs 曾经又套了一层 scope(\"/ws\") -> 实际变成 /v1/ws/ws"
        );
    }

    // 反向锁定: 修复后不应同时存在重复路径。留着这条是为了让「修好了」与
    // 「把 scope 挪到别处又叠出第二个入口」区分开。
    let req = actix_web::test::TestRequest::get()
        .uri("/v1/ws/ws")
        .to_request();
    let status = actix_web::test::call_service(&app, req).await.status();
    assert_eq!(
        status.as_u16(),
        404,
        "/v1/ws/ws 仍在路由表上 —— 说明 scope 被叠了两层, 而非只被修好了一半"
    );
}

/// actix 嵌套 scope 的语义探针 —— **不依赖 PG**
///
/// 这条不测我们的代码, 只测 actix: 两个 `scope("/ws")` 套在一起会不会塌成
/// 一个? **会** —— actix 的 scope 是嵌套的, 前缀逐层相加。写下它是因为
/// 「看起来重复的 scope 会不会自动合并」是个很容易靠直觉答错的问题, 而
/// 本仓库正好踩了这个坑。
///
/// 用桩 handler 复刻同一层嵌套即可, 不需要真实 `AppState`。
#[actix_web::test]
async fn nested_scopes_of_the_same_prefix_still_nest() {
    async fn stub() -> &'static str {
        "stub"
    }
    let app = actix_web::test::init_service(actix_web::App::new().service(
        web::scope("/v1").service(web::scope("/ws").configure(|cfg: &mut web::ServiceConfig| {
            cfg.service(web::scope("/ws").route("", web::get().to(stub)));
        })),
    ))
    .await;

    let hit = |p: &'static str| {
        let app = &app;
        async move {
            actix_web::test::call_service(
                app,
                actix_web::test::TestRequest::get().uri(p).to_request(),
            )
            .await
            .status()
            .as_u16()
        }
    };
    assert_eq!(
        hit("/v1/ws").await,
        404,
        "同名前缀的 scope **不会**自动合并"
    );
    assert_eq!(hit("/v1/ws/ws").await, 200, "两层同名前缀 = 路径叠加两次");
}

/// 首帧必须是 `auth` —— 未鉴权连接不得放行业务帧
///
/// `aux-13 §1.1.1`: 客户端 connect 后必须第一帧发 `auth`, 否则服务端主动断开。
/// 没有这条的话, 未鉴权连接能直接发 `send_message`。
#[actix_web::test]
async fn first_frame_must_be_auth() {
    let (h, f) = hz!();
    let mut c = TestWs::connect(h.addr, "/v1/ws").await.expect("connect");

    // 首帧发业务帧(ping 是最轻的合法帧, 便于隔离「鉴权前一律拒绝」这一条)
    c.send_text(&serde_json::json!({"type":"ping","ts":1}).to_string())
        .expect("send");

    let resp = c
        .next_text(Duration::from_secs(5))
        .await
        .expect("服务端应先回一个错误帧再断开, 而不是默默丢弃");
    let v: serde_json::Value =
        serde_json::from_str(&resp).unwrap_or_else(|e| panic!("错误帧不是 JSON: {resp} ({e})"));
    let blob = v.to_string();
    assert!(
        blob.contains("VALIDATION_ERROR") || blob.contains("first frame must be Auth"),
        "首帧非 auth 应回 VALIDATION_ERROR, 实际: {v}"
    );
    let _ = f;
}

/// 鉴权后 `ping` 必须被回 `pong`, 且 ts 原样回传
///
/// 这是一条**真实回归**: 此前 handler 里根本没有 `ClientFrame::Ping` 分支,
/// ping 落进 `Ok(_)` 兜底被当成业务帧回 `VALIDATION_ERROR` —— 客户端发 ping
/// 永远收不到 pong, 而模块文档写的是「回 PongFrame(ts)」。
#[actix_web::test]
async fn ping_after_auth_gets_pong_with_the_same_ts() {
    let (h, f) = hz!();
    let mut c = authed(&h, &f.alice_token).await.expect("auth");

    c.send_text(&serde_json::json!({"type":"ping","ts":4242}).to_string())
        .expect("send ping");
    let pong = c.next_json(Duration::from_secs(5)).await.expect("pong");
    assert_eq!(
        pong.get("type").and_then(|t| t.as_str()),
        Some("pong"),
        "ping 应回 pong, 实际: {pong}"
    );
    assert_eq!(
        pong.get("ts").and_then(|t| t.as_i64()),
        Some(4242),
        "pong 的 ts 必须原样回传, 实际: {pong}"
    );
}

/// 广播只送到**会话成员**: Bob(成员)收到, Carol(非成员)收不到
///
/// 这是 WS 侧最要紧的一条不变量。若 `should_deliver` 退化成「广播给所有人」,
/// 本用例的 Carol 就会收到 —— 而**用户看到的现象是「群里的话出现在别人那儿」**,
/// 属于跨会话泄漏, 不是「功能不好」。
#[actix_web::test]
async fn broadcast_reaches_other_members_but_never_a_non_member() {
    let (h, f) = hz!();

    let mut alice = authed(&h, &f.alice_token).await.expect("alice auth");
    let mut bob = authed(&h, &f.bob_token).await.expect("bob auth");
    let mut carol = authed(&h, &f.carol_token).await.expect("carol auth");

    // Alice 在 conv 里发消息; Bob 是该会话成员, Carol 不是
    let req_id = uuid::Uuid::new_v4();
    alice
        .send_text(
            &serde_json::json!({
                "type": "send_message",
                "req_id": req_id,
                "conversation_id": f.conv.0,
                "idempotency_key": uuid::Uuid::new_v4(),
                "kind": "text",
                "content": {"kind": "text", "text": "ws e2e"},
            })
            .to_string(),
        )
        .expect("send_message");

    // **先确认发送方拿到了成功的 ack**, 再去查广播。
    //
    // 少了这一步, 「Bob 没收到」这句话有两种完全不同的成因:
    //   (a) Alice 的 send_message 本身被拒(成员校验/序列号/内容 schema)
    //   (b) 消息写进去了, 但广播没送达
    // 而这两者的排查方向毫不相干 —— (a) 查 MessageService, (b) 查 WsHub。
    // 一条分不清这两者的失败信息会把人直接引到错的地方。
    let ack = alice
        .next_json(Duration::from_secs(10))
        .await
        .expect("Alice 发送后应收到 ack");
    assert_eq!(
        ack.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "send_message 本身必须成功, 否则下面的广播断言无从谈起。收到: {ack}"
    );
    assert_eq!(
        ack.get("req_id").and_then(|v| v.as_str()),
        Some(req_id.to_string().as_str()),
        "ack 必须回显 req_id, 否则无法确认这个 ack 是本次发送的。收到: {ack}"
    );

    // Bob 必须收到
    let got = bob
        .next_json(Duration::from_secs(10))
        .await
        .expect("bob 应收到广播");
    assert_eq!(
        got.get("type").and_then(|t| t.as_str()),
        Some("message_new"),
        "Bob 是会话成员, 应收到 message_new, 实际: {got}"
    );
    let msg_conv = got
        .get("message")
        .and_then(|m| m.get("conversation_id"))
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    assert_eq!(
        msg_conv,
        f.conv.0.to_string(),
        "广播里的 conversation_id 必须是发起的那一个"
    );

    // Carol 收不到 —— 且**必须等满**一段时间才能下结论。
    // 只等 100ms 的话, 「服务端只是还没广播」与「服务端正确地没广播」
    // 观察上完全一样, 那是假绿灯。
    carol
        .expect_silence(Duration::from_secs(2))
        .await
        .unwrap_or_else(|e| panic!("Carol 不是会话成员, 不该收到任何帧: {e}"));
}

/// 鉴权失败必须被拒, 且不得进入业务态
#[actix_web::test]
async fn bad_token_is_rejected() {
    let (h, _f) = hz!();
    let mut c = TestWs::connect(h.addr, "/v1/ws").await.expect("connect");
    c.send_text(&serde_json::json!({"type":"auth","access_token":"not-a-real-token"}).to_string())
        .expect("send");

    let resp = c
        .next_text(Duration::from_secs(5))
        .await
        .expect("应回一个错误帧");
    let v: serde_json::Value =
        serde_json::from_str(&resp).unwrap_or_else(|e| panic!("错误帧不是 JSON: {resp} ({e})"));
    assert_ne!(
        v.get("type").and_then(|t| t.as_str()),
        Some("auth_ok"),
        "假 token 绝不能拿到 auth_ok, 实际: {v}"
    );
}

/// 握手用的示例 key 必须是合法的 base64, 否则服务端会拒
///
/// RFC 6455 §4.1 要求 `Sec-WebSocket-Key` 解码后为 16 字节。写成断言而不是
/// 注释: 改坏这个常量的话, 症状是「所有 WS 用例莫名其妙全红」, 而报错信息
/// 只会说 upgrade 被拒 —— 与真正原因之间没有任何可追踪关联。
#[test]
fn rfc6455_example_key_is_wellformed_base64() {
    // "the sample nonce" 的 base64 = 24 字符(解码 16 字节)
    assert_eq!(WS_KEY.len(), 24, "示例 key 的 base64 长度");
    assert!(WS_KEY.is_ascii(), "base64 必须是 ASCII");
}
