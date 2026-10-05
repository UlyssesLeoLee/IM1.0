//! OpenAPI 规范 ↔ actix 路由表**运行时**契约测试
//!
//! 依据: `docs/api/openapi.json` 自称「与代码路由注册双向比对」。本文件负责其中
//! **一个方向**: 规范 → 代码。即「规范里写了的, actix 里必须真的有」。
//!
//! ## 为什么必须**跑一次真路由器**, 而不能只做静态比对
//!
//! 静态检查(如 grep 路由字符串、或比对 path 模板)挡不住一类漂移: 有人改了
//! `http/mod.rs` 的注册却没改规范, 或反之。两边各自「自洽」, 静态比对看不出来。
//! 所以这里对每条 (method, path) 发**一个真请求**, 让 actix 自己的匹配逻辑回答
//! 「这条路由存在吗」。
//!
//! ## 判别式: `404 且 body 为空` == 路由未命中
//!
//! 这不是约定, 是本仓的形状事实, 两侧都有依据:
//!
//! - actix 路由未命中 → `404` + **空 body**(`HttpResponse::NotFound().finish()`)
//! - 本仓 handler 的一切错误(含 404)都走
//!   `http/error_response.rs::json_response`, 它**总是** `.json(body)` ——
//!   即 handler 级 404 一律带 JSON 错误信封(`code`/`message`/`trace_id`/`ts`)
//!
//! 故「404 + 空 body」唯一地对应「没有这条路由」。`unrouted = 404 && 空 body`。
//!
//! ## 这个判别式会失效 —— 所以有对照组
//!
//! 若哪天有人给 actix 装了个自定义 404 页(带 body), 或让某个 handler 返回
//! 裸 `NotFound().finish()`, 上面那条判别式就退化成**恒真**: 所有未被路由的
//! path 都会「通过」, 测试变成废物而不报任何错。
//! `undocumented_path_is_not_routed` 就是这个退化机制的报警器 —— 它断言一个
//! 规范里没有的 path **确实**返回「404 + 空 body」。它先红, 而不是让漂移静悄悄
//! 地溜过去。
//!
//! ## 无跳过路径
//!
//! 本文件**刻意不复用** `test_support::e2e_pool()`: 它连不上 PG 就静默 skip,
//! 而 skip 在 libtest 眼里**等于通过**(`test_support.rs` 自己的长注释记录了
//! 一次「6 个全通过、实际 6 个全跳过」的事故)。本测试的价值完全建立在「它真的
//! 跑过」之上, 因此**没有任何 skip 分支**。
//!
//! 数据库用 `connect_lazy` 指向一个连不上的端口: 它不建连接, 于是
//! `AppState` 需要的 7 个 service 全部能构造出来而不碰网络(与
//! `health.rs::tests::dead_pool` 同一手法)。路由是否命中与业务是否成功无关 ——
//! handler 返 400/401/500 都在预期之内。

#![cfg(test)]

use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use actix_web::http::{Method, StatusCode};
// **刻意不写** `use actix_web::test`。
//
// `actix_web::test` 既是模块也是**属性宏**(`#[actix_web::test]`), 而 `use`
// 会把被导入项所占的**所有**命名空间一并带进来 —— 包括宏命名空间。于是本文件里
// 的 `#[test]` 不再解析到 Rust 内置属性, 而是解析到 `actix_web::test`, 而后者
// **要求函数体是 `async fn`**。于是出现一条极难定位的报错:
//
//     error: the async keyword is missing from the function declaration
//      --> openapi_contract.rs:459:1
//       |
//     459 | fn route_table_baseline() {
//       | ^^
//
// 它指向一个**函数体里根本没有 `.await`** 的同步函数, 且把函数体换成
// `let _ = 1 + 1;` 之后照报不误 —— 与函数体无关, 纯粹是属性解析到了错误的宏。
//
// 本仓其余 14 个 .rs 全部用全限定的 `actix_web::test::...` 且从不裸导入 `test`,
// 所以只有这个文件中招。此处同样改用全限定形式, 与其余文件保持一致。
use actix_web::{web, App};
use serde_json::Value;

use super::state::AppState;

/// 仓内 OpenAPI 3.1 规范(4 层上溯 = 仓根)
const SPEC_JSON: &str = include_str!("../../../../docs/api/openapi.json");

/// OpenAPI 里构成 operation 的 key 全集。
/// `parameters` / `summary` / `description` / `servers` 等**不是** operation,
/// 遍历时必须按这张白名单过滤, 否则会把它们当成路由发请求。
const OPERATION_KEYS: [&str; 8] = [
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

/// 规范里的路径参数(`{id}` / `{msg_id}`)统一替换成的哑 UUID。
///
/// 必须是**合法** UUID: 多数 handler 把它 parse 成 id 类型, parse 失败会走
/// 400/404 之外的分支, 平白多一层不确定性。业务上它是**不存在的** id
/// (真库连不上), 所以 handler 只会走「鉴权失败 / 找不到」这些**带 body** 的分支。
const DUMMY_ID: &str = "00000000-0000-4000-8000-000000000001";

/// 故意连不上的 PG(端口 1 上不会有 PG)。`connect_lazy` 不建连接。
const UNREACHABLE_DATABASE_URL: &str = "postgres://nobody:nobody@127.0.0.1:1/none";

/// 对照组探针路径: 规范里**没有**, 代码里也**不该有**。
const CONTROL_PATH: &str = "/v1/__openapi_probe_control__";

// ---------------------------------------------------------------------------
// 手工核对过的基线 (不是猜的; 见 route_table_baseline 的计数说明)
// ---------------------------------------------------------------------------

/// `paths` 的键数。
///
/// 数法: 把 `docs/api/openapi.json` 的 `paths` 对象逐条展开 ——
/// 根级 3 条 (`/healthz` / `/readyz` / `/metrics`, 对应 `main.rs` 挂在 App 根的
/// 那三个) + `/v1` 下 18 条 (auth 5 + conversations 8 + friends 3 + me 1 + ws 1)
/// = **21**。
const EXPECTED_PATH_COUNT: usize = 21;

/// operation 总数(= 24)。
///
/// 数法: 每条 path 下面**属于 `{get,post,put,patch,delete,head,options,trace}`
/// 白名单的 key** 各算一个 operation。21 条 path 里 3 条是**双 method**
/// (`/v1/conversations` = post+get, `/v1/conversations/{id}/messages` = post+get,
/// `/v1/me` = get+patch), 其余 18 条各 1 个, 于是
/// `18 × 1 + 3 × 2 = 24`。
///
/// 另一条独立算法(逐条相加, 用于交叉验证): 根级 3 + auth 5 + conversations
/// (2+1+1+1+2+1+1+1 = 10) + friends 3 + me 2 + ws 1 = 3+5+10+3+2+1 = **24**。
const EXPECTED_OPERATION_COUNT: usize = 24;

/// 上面 24 = 18×1 + 3×2 里的那 3 条双 method path(排序后)。
const EXPECTED_DUAL_METHOD_PATHS: [&str; 3] = [
    "/v1/conversations",
    "/v1/conversations/{id}/messages",
    "/v1/me",
];

/// `main.rs:243-252` 把这三个挂在 **App 根**(`/v1` scope 之外), 规范里也必须
/// 写在 `/v1` 之外 —— 否则说明规范与装配结构已经各说各话。
const EXPECTED_ROOT_PATHS: [&str; 3] = ["/healthz", "/metrics", "/readyz"];

// ---------------------------------------------------------------------------
// 规范解析
// ---------------------------------------------------------------------------

struct Operation {
    method: String,
    /// 规范里的原始 path(带 `{id}` 之类的占位符)
    path: String,
}

fn spec_value() -> Value {
    serde_json::from_str(SPEC_JSON).expect("docs/api/openapi.json 必须是合法 JSON")
}

/// `paths` 对象(按 BTreeMap 序, 保证测试输出稳定)
fn spec_paths(spec: &Value) -> &serde_json::Map<String, Value> {
    spec.get("paths")
        .and_then(Value::as_object)
        .expect("规范必须含顶层 paths 对象")
}

fn is_operation_key(key: &str) -> bool {
    OPERATION_KEYS.contains(&key)
}

/// 规范里全部 (method, path), 按 (path, method) 排序 —— 排序只为让失败输出可
/// 复现, 不影响结论。
fn documented_operations() -> Vec<Operation> {
    let spec = spec_value();
    let mut ops = Vec::new();
    for (path, item) in spec_paths(&spec) {
        let item = item
            .as_object()
            .unwrap_or_else(|| panic!("paths./{path} 必须是对象"));
        for (key, _) in item {
            // 白名单过滤: `parameters` / `summary` / `description` 等一律跳过
            if !is_operation_key(key) {
                continue;
            }
            ops.push(Operation {
                method: key.clone(),
                path: path.clone(),
            });
        }
    }
    ops.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.method.cmp(&b.method)));
    ops
}

/// `{id}` / `{msg_id}` → 哑 UUID, 得出 actix 意义上的具体 path。
///
/// **只发规范里写的那一条**, 不加尾斜杠之类的变体。本仓里尾斜杠确实存在但不在
/// 规范里: `ws/router.rs:13-14` 给 `/v1/ws` 同时注册了 `""` 与 `"/"`。那是
/// 「代码多注册了一条」—— 属于**另一个方向**的漂移(代码 → 规范), 不在本测试
/// 职责内, 也不该由本测试替规范所有者决定尾斜杠算不算同一个端点。
/// `conversations` 那组(`http/mod.rs:71-72`)则是同一路径的两个 method,
/// 规范里 `post`/`get` 两条 operation 各自对应一个, 正好对上。
fn concrete_path(path: &str) -> String {
    let mut out = path.to_string();
    while let Some(start) = out.find('{') {
        let Some(rel_end) = out[start..].find('}') else {
            break;
        };
        out.replace_range(start..start + rel_end + 1, DUMMY_ID);
    }
    out
}

// ---------------------------------------------------------------------------
// 夹具: 真路由 + 永不可达的 DB(不连库、不跳过)
// ---------------------------------------------------------------------------

/// 构造一个**连接永远建不起来**的池。
///
/// 用 `connect_lazy` 而非 `connect`: 后者对不可达端口当场返回
/// `Err(PoolTimedOut)`, 连 `AppState` 都构造不出来。与 `health.rs::tests::dead_pool`
/// 同一手法。
fn dead_pool() -> sqlx::PgPool {
    sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_millis(200))
        .connect_lazy(UNREACHABLE_DATABASE_URL)
        .expect("connect_lazy 不发起连接, 必然成功")
}

/// 装配与 `main.rs` **同构**的三个注入物(7 个 service + pool + hub)。
///
/// service 的类型/顺序逐个对照 `main.rs:150-219` 与 `test_support.rs:200-305`。
/// 全部仓储都只**持有** pool, 构造时不发查询, 故此处无网络 I/O。
fn app_parts() -> (AppState, sqlx::PgPool, crate::ws::hub::WsHub) {
    let pool = dead_pool();

    let conversation_repo: Arc<dyn im_core::conversation::repository::ConversationRepository> =
        Arc::new(im_core::conversation::pg::PgConversationRepository::new(
            pool.clone(),
        ));
    let message_repo: Arc<dyn im_core::message::repository::MessageRepository> =
        Arc::new(im_core::message::pg::PgMessageRepository::new(pool.clone()));
    let sequencer: Arc<dyn im_core::message::sequence::SequenceAllocator> =
        Arc::new(im_core::message::pg::PgSequenceAllocator::new(pool.clone()));
    // 显式 stub, 不借道 NatsEventPublisher(后者一旦真去连会变成 5s 超时假失败,
    // 见 test_support.rs:203-208 的记录)
    let events: Arc<dyn im_core::event::publisher::EventPublisher> =
        Arc::new(im_core::event::publisher::StubEventPublisher::new());

    let token_service = Arc::new(im_core::identity::token::TokenService::new(
        vec![im_core::identity::token::SigningKey {
            kid: "v1".into(),
            key: secrecy::SecretString::new("openapi-contract-key-must-be-32-bytes-padding".into()),
        }],
        chrono::Duration::seconds(900),
        secrecy::SecretString::new("openapi-contract-pepper".into()),
    ));

    // 空 map 即可: 本测试不跑任何业务, 只验路由是否存在, 不需要签名密钥。
    let server_secrets: std::collections::HashMap<
        im_common::ids::EnvironmentId,
        secrecy::SecretString,
    > = std::collections::HashMap::new();

    let state = AppState::new(
        Arc::new(im_core::conversation::service::ConversationService::new(
            conversation_repo.clone(),
        )),
        Arc::new(im_core::message::service::MessageService::new(
            message_repo.clone(),
            sequencer,
            events,
            conversation_repo.clone(),
        )),
        token_service.clone(),
        Arc::new(im_core::identity::service::IdentityService::new(
            im_core::identity::pg::PgUserRepository::new(pool.clone()),
            im_core::identity::pg::PgDeviceSessionRepository::new(pool.clone()),
            token_service,
            server_secrets,
        )),
        Arc::new(im_core::settings::service::SettingsService::new(
            pool.clone(),
        )),
        Arc::new(im_core::reaction::service::ReactionService::new(
            Arc::new(im_core::reaction::pg::PgReactionRepository::new(
                pool.clone(),
            )),
            message_repo,
            conversation_repo,
        )),
        Arc::new(im_core::relationship::service::RelationshipService::new(
            Arc::new(im_core::relationship::pg::PgFriendshipRepository::new(
                pool.clone(),
            )),
        )),
    );

    (state, pool, crate::ws::hub::WsHub::new())
}

/// 搭出与 `main.rs:243-252` 结构一致的 App。
///
/// 写成宏而不是返回 `impl Service<...>`: 那个返回类型需要 `actix_http` 在作用域里,
/// 而本 crate 没有直接依赖它。用宏则两个测试**共用同一份装配代码** —— 路由结构
/// 只可能有一份, 不存在「两个测试各搭一份、悄悄搭得不一样」。
macro_rules! openapi_router {
    () => {{
        let (state, pool, ws_hub) = app_parts();
        App::new()
            .app_data(web::Data::new(state))
            .app_data(web::Data::new(pool))
            .app_data(web::Data::new(ws_hub))
            .service(web::scope("/v1").configure(crate::http::configure))
            .route("/healthz", web::get().to(crate::health::healthz))
            .route("/readyz", web::get().to(crate::health::readyz))
            .route("/metrics", web::get().to(crate::health::metrics))
    }};
}

// ---------------------------------------------------------------------------
// 探测
// ---------------------------------------------------------------------------

struct Probe {
    /// 规范里的 method(小写), 如 `post`
    method: String,
    /// 实际请求的 path(占位符已替换)
    uri: String,
    status: StatusCode,
    /// `actix_web::test::read_body` 返回的是 `web::Bytes` 而不是 `Vec<u8>`
    /// —— 2026-10-05 首次 CI 编译撞在这里(E0308: expected `Vec<u8>`, found
    /// `Bytes`)。`Bytes` 对 `&[u8]` 有 `Deref`, 故 `from_utf8_lossy(&self.body)`
    /// 与 `.len()` / `.is_empty()` 都不必改。
    body: actix_web::web::Bytes,
}

impl Probe {
    /// 本仓的判别式: 404 + 空 body == actix 路由表里没有这条路由
    fn unrouted(&self) -> bool {
        self.status == StatusCode::NOT_FOUND && self.body.is_empty()
    }

    /// 失败信息用的一行: method + path + 实际 status + body 前 200 字符
    fn describe(&self) -> String {
        let preview: String = String::from_utf8_lossy(&self.body)
            .chars()
            .take(200)
            .collect();
        format!(
            "  {:<6} {:<62} -> {} | body(前200字符, {} 字节): {:?}",
            self.method.to_uppercase(),
            self.uri,
            self.status.as_u16(),
            self.body.len(),
            preview
        )
    }
}

/// 对一条 (method, path) 发一个真请求, 返回 status + body。
///
/// 期望的 status 几乎必然是 400/401/500(没 token、body 没字段、连不上库)——
/// **没关系**: 本测试要证明的是「路由命中了」, 不是「业务成功」。业务语义由各
/// handler 自己的 e2e 负责。
///
/// 写成宏而非 `async fn`: 那个签名得写出 actix 的请求类型
/// (`impl Service<actix_http::Request, ..>`), 而本 crate **没有**直接依赖
/// `actix_http`, 写出来编译不过。宏在调用点就地展开, 类型自然推导得出。
macro_rules! probe {
    ($app:expr, $method:expr, $uri:expr) => {{
        let method = Method::from_str($method).unwrap_or_else(|e| {
            panic!("规范里的 method {:?} 不是合法 HTTP method: {e}", $method)
        });
        // 必须在 `.method(method)` 之前取好: 那是按值传参, 之后 `method` 已被移走。
        let method_name = method.as_str().to_ascii_lowercase();
        let req = actix_web::test::TestRequest::default()
            .method(method)
            .uri($uri)
            // 统一发 application/json 的 {}: 让所有 handler 都走到「已命中」的
            // 业务分支, 而不是死在 extract 阶段。
            .set_json(serde_json::json!({}))
            .to_request();
        let resp = actix_web::test::call_service(&$app, req).await;
        let status = resp.status();
        let body = actix_web::test::read_body(resp).await;
        Probe {
            method: method_name,
            uri: $uri.to_string(),
            status,
            body,
        }
    }};
}

// ---------------------------------------------------------------------------
// 测试 1: 规范里每条 operation 都必须在 actix 路由表里
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn every_documented_operation_routes() {
    let ops = documented_operations();
    assert!(
        !ops.is_empty(),
        "规范里一条 operation 都没有 —— openapi.json 的 paths 是不是被清空了?"
    );

    let app = actix_web::test::init_service(openapi_router!()).await;

    // 逐条探测; 收集全部结果而不是遇错即停 —— 一次跑完才知道漂移是 1 条还是 12 条。
    let mut probes = Vec::with_capacity(ops.len());
    for op in &ops {
        probes.push(probe!(&app, &op.method, &concrete_path(&op.path)));
    }

    let unrouted: Vec<&Probe> = probes.iter().filter(|p| p.unrouted()).collect();
    assert!(
        unrouted.is_empty(),
        "OpenAPI 与 actix 路由表漂移: 规范里 {total} 条 operation, 其中 {bad} 条在代码里**不存在**\n\
         (判别式: status==404 且 body 为空 = actix 路由未命中)\n\
         未命中的 operation:\n{detail}\n\
         \n\
         全部 {total} 条探测结果(method + path + 实际 status + body 前 200 字符):\n{all}\n\
         \n\
         两种修法: 改代码注册路由, 或改规范。**不要**为了让测试变绿去动本文件。",
        total = probes.len(),
        bad = unrouted.len(),
        detail = unrouted
            .iter()
            .map(|p| p.describe())
            .collect::<Vec<_>>()
            .join("\n"),
        all = probes
            .iter()
            .map(|p| p.describe())
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

// ---------------------------------------------------------------------------
// 测试 2: 对照组 —— 规范里没有的 path 必须「404 + 空 body」
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn undocumented_path_is_not_routed() {
    // 探针路径必须真的**不在**规范里。否则对照组测的是「规范里的路由」,
    // 判别式的鉴别力就无从谈起 —— 宁可这里红, 也不要一个悄悄失效的对照组。
    assert!(
        !documented_operations()
            .iter()
            .any(|op| op.path == CONTROL_PATH),
        "对照组失效: 规范里出现了探针路径 {CONTROL_PATH}。换个探针路径。"
    );

    let app = actix_web::test::init_service(openapi_router!()).await;

    for method in [Method::GET, Method::POST] {
        let p = probe!(&app, method.as_str(), CONTROL_PATH);
        assert_eq!(
            p.status,
            StatusCode::NOT_FOUND,
            "{CONTROL_PATH} ({}) 应当返回 404, 实际 {}。\n\
             它既不在规范里也不在代码里 —— 若这里命中了路由, 说明有人注册了它。\n{}",
            method,
            p.status.as_u16(),
            p.describe(),
        );
        assert!(
            p.body.is_empty(),
            "判别式失效: 未命中的路由应当返回**空** body, 实际拿到 {} 字节: {}\n\
             —— 只要这里有内容, `404 且空 body` 就不再等价于「路由未命中」, \
             测试 1 就退化成恒真的废物断言。\n{}",
            p.body.len(),
            String::from_utf8_lossy(&p.body)
                .chars()
                .take(200)
                .collect::<String>(),
            p.describe(),
        );
    }
}

// ---------------------------------------------------------------------------
// 测试 3: 规范自身的基线(手工核对值)
// ---------------------------------------------------------------------------

#[test]
fn route_table_baseline() {
    let spec = spec_value();
    let paths = spec_paths(&spec);

    assert_eq!(
        paths.len(),
        EXPECTED_PATH_COUNT,
        "规范里 paths 的键数从 {EXPECTED_PATH_COUNT} 变成了 {}。\
         有意变更时请同步改本文件的 EXPECTED_PATH_COUNT / EXPECTED_OPERATION_COUNT, \
         并在 commit 里说明多/少了哪一条。",
        paths.len()
    );

    // 直接对 paths 的每个 value 数 operation, 不借用任何中间集合。
    let mut op_count = 0usize;
    for item in paths.values() {
        if let Some(obj) = item.as_object() {
            op_count += obj.keys().filter(|k| is_operation_key(k)).count();
        }
    }
    assert_eq!(
        op_count, EXPECTED_OPERATION_COUNT,
        "规范里 operation 总数从 {EXPECTED_OPERATION_COUNT} 变成了 {op_count}。\
         数量不变但内容变了是**更危险**的漂移(总数对得上) —— 那由 every_documented_operation_routes 兜。"
    );

    // 把 24 的算式**钉在代码里**: 21 条 path 里究竟哪几条是双 method。
    // 只断言总数的话, 「多一条单 method + 少一条双 method」这种对冲能蒙混过关。
    let mut dual: Vec<&str> = Vec::new();
    for (path, item) in paths {
        let n = item
            .as_object()
            .map(|o| o.keys().filter(|k| is_operation_key(k)).count())
            .unwrap_or(0);
        if n > 1 {
            dual.push(path.as_str());
        }
    }
    dual.sort_unstable();
    let expected_dual: Vec<&str> = EXPECTED_DUAL_METHOD_PATHS.to_vec();
    let expected_dual_sorted = {
        let mut v = expected_dual;
        v.sort_unstable();
        v
    };
    assert_eq!(
        dual, expected_dual_sorted,
        "双 method 的 path 集合变了(3 条 x 2 + 其余 18 条 x 1 = 24)。当前: {dual:?}"
    );

    // 顺带把「根级 3 条 + 其余都在 /v1 下」这个装配事实钉住(main.rs:243-252):
    // 规范若把 /healthz 挪进 /v1, 说明它与实际装配结构已经各说各话。
    let mut root: Vec<&str> = paths
        .keys()
        .filter(|p| !p.starts_with("/v1/"))
        .map(String::as_str)
        .collect();
    root.sort_unstable();
    let mut expected_root: Vec<&str> = EXPECTED_ROOT_PATHS.to_vec();
    expected_root.sort_unstable();
    assert_eq!(
        root, expected_root,
        "规范里 /v1 之外的 path 应恰好是 main.rs 挂在 App 根的那三条"
    );
}
