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
/// ## 为什么是带显式返回类型的 `fn`, 而不是宏
///
/// 初版写成 `macro_rules!`, 在调用点就地展开 `App::new()...`。**结果是 24 条
/// operation 全部返回 `404 + 空 body`** —— 也就是这个 App **一条路由都没装上**。
/// 原因: 宏在 `test::init_service(openapi_app()).await` 的位置就地展开时,
/// `App::new()` 的类型参数 `T` 没有被任何东西钉住, 被推断成了别的 `T`;
/// 而 `App<T>` 的路由是随 `T` 的 `ServiceFactory` 一起被组装的, `T` 一旦不是
/// 预期的那个, 注册上去的 service 就不生效。
///
/// 显式写出返回类型 `App<impl ServiceFactory<ServiceRequest, Config = (), ...>>`
/// 就把 `T` 钉死了。**这正是本仓 `friends.rs::tests::friends_app` 用的写法**
/// (它也有一大段注释解释为什么需要这个返回类型), 与邻居保持一致。
///
/// 注: 需要显式返回类型的是 **App 本身**, 不是 `init_service` 的产物 ——
/// 后者确实要 `actix_http` 才能命名, 但那与本函数无关。
fn openapi_app() -> actix_web::App<
    impl actix_web::dev::ServiceFactory<
        actix_web::dev::ServiceRequest,
        Config = (),
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
        InitError = (),
    >,
> {
    let (state, pool, ws_hub) = app_parts();
    App::new()
        .app_data(web::Data::new(state))
        .app_data(web::Data::new(pool))
        .app_data(web::Data::new(ws_hub))
        .service(web::scope("/v1").configure(crate::http::configure))
        .route("/healthz", web::get().to(crate::health::healthz))
        .route("/readyz", web::get().to(crate::health::readyz))
        .route("/metrics", web::get().to(crate::health::metrics))
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
/// ## 必须先把 method 转成大写 —— 否则 24 条 operation 会「全部未命中」
///
/// 规范里 method 写作小写 (`get` / `post`), 而 `http::Method::from_str` 对
/// 标准方法是**大小写敏感**的。匹配不上时它**不报错**, 而是按
/// `Method::from_bytes` 造出一个**小写的自定义 method**。于是:
///
/// - 请求带的是自定义 method `get`
/// - 路由表里注册的是标准 method `GET`
/// - actix 判为 method 不匹配 -> `404 + 空 body`
///
/// 结果是 **24 条 operation 一条不中, 连 `/healthz` 这种直接注册在 App 根上的
/// 路由也不中**。而这个现象与「规范漏写了路由」长得一模一样, 极易误判成漂移。
///
/// 顺带说明: `undocumented_path_is_not_routed` 用的是字面量 `Method::GET` /
/// `Method::POST`(本身就是大写常量), 所以它**不受**此影响, 在 App 为空时也照样
/// 通过 —— 它守的是判别式的退化, 守不了 method 拼写。
macro_rules! probe {
    ($app:expr, $method:expr, $uri:expr) => {{
        // 规范里是小写; 必须在 from_bytes **之前**转大写。
        let upper = $method.to_ascii_uppercase();
        let method = Method::from_bytes(upper.as_bytes()).unwrap_or_else(|e| {
            panic!("规范里的 method {:?} 不是合法 HTTP method: {e}", $method)
        });
        // 展示用: 规范里的原始小写形态。必须在 `.method(method)` 之前取好 ——
        // 那是按值传参, 之后 `method` 已被移走。
        let method_name = $method.to_string();
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

    let app = actix_web::test::init_service(openapi_app()).await;

    // 逐条探测; 收集全部结果而不是遇错即停 —— 一次跑完才知道漂移是 1 条还是 12 条。
    let mut probes = Vec::with_capacity(ops.len());
    for op in &ops {
        probes.push(probe!(&app, &op.method, &concrete_path(&op.path)));
    }

    let unrouted: Vec<&Probe> = probes.iter().filter(|p| p.unrouted()).collect();

    // 阳性对照: 若**全部** operation 都未命中, 那几乎不可能是「规范漏写了路由」——
    // 真实漂移总是零星几条。更可能的是这个 App 本身没把任何路由装上(method 拼写
    // 错了、装配没生效等)。这两种失败在探测结果上**长得一模一样**, 2026-10-05
    // 就因为没区分它们, 把一个测试自身的缺陷误读成了「规范与代码全面漂移」,
    // 白白排查了 App 的类型参数。分开报, 下一个人不必重走这条路。
    assert!(
        unrouted.len() != probes.len(),
        "全部 {n} 条 operation 都未命中 —— 这**不是**规范漂移, 而是这个测试 App \
         没能提供任何可命中的路由(请求的 method/path 没匹配上, 或装配未生效)。\
         先怀疑本文件, 再怀疑规范。\n全部探测结果:\n{all}",
        n = probes.len(),
        all = probes
            .iter()
            .map(|p| p.describe())
            .collect::<Vec<_>>()
            .join("\n"),
    );

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

    let app = actix_web::test::init_service(openapi_app()).await;

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

// ---------------------------------------------------------------------------
// 测试 4: /metrics 的 description 与真实输出必须对得上
// ---------------------------------------------------------------------------

/// `/metrics` 真实暴露的指标条数(2026-10-05 手工核对)。
///
/// 4 条 WS(`im_ws_*`) + 3 条领域事件(`im_events_*`)。数法: `health.rs::metrics`
/// 里 `# HELP` 注释行逐条数 —— 每个指标恰好一条。
const EXPECTED_METRIC_COUNT: usize = 7;

/// 从 markdown 反引号跨度里挑出形如 `` `im_xxx` `` 的**指标名**。
///
/// ## 为什么要自己写, 而不用正则库
///
/// `/metrics` 的 description 是**给人读的散文**, 不是结构化字段 —— 它同时含
/// 指标名、环境变量名、URL。所以「哪些反引号跨度算指标名」必须被一条明确
/// 写死的规则回答, 而不是让某个正则的运气决定。这个规则是三段合取:
/// ① 在反引号里(按 `` ` `` 切开后取奇数下标) ② 以 `im_` 开头 ③ 剩余字符全部
/// 是 `[a-z0-9_]`。
///
/// 第 ③ 条同时排掉两类真实存在的干扰项:
/// - `` `IM_EVENT_PUBLISHER_KIND` `` —— 环境变量名, **大写**, 必须被排除
/// - `` `GET /v1/conversations/{id}/messages` `` —— 含斜杠与大写字母, 被排除
///
/// 两条都**不是**假想敌: 它们此刻就写在同一段 description 里。规则若只写
/// 「以 im_ 开头」, 大写的那个已经匹配不到了(PowerShell/JS 的 startsWith 之类
/// 默认大小写敏感), 但含 `/` 和 `{}` 的那个会被放过 —— 故 ③ 不可省。
///
/// ## 反引号个数为奇数(没闭合)会怎样
///
/// 下标奇偶会整体错位, 抽出来的名字变成乱码。此时测试 4 的**双向**集合比较
/// 仍然会红(真实集合是 7 个已知名字), 所以这是个响亮的失败, 不是静悄悄的漏检。
fn backticked_metric_names(text: &str) -> Vec<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| {
            span.len() > 3
                && span.starts_with("im_")
                && span
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .map(str::to_string)
        .collect()
}

/// 从真实 `/metrics` 响应体里取出**数据行**的指标名。
///
/// 跳过 `#` 开头的注释行(`# HELP` / `# TYPE` / 顶部那行 MVP 说明) —— 它们是
/// 元信息, 名字会在同一指标上重复出现两三次, 混进来集合就不对了。
/// 再按 `im_` 前缀滤一道, 免得将来有人在末尾加一行自由格式的说明文字。
fn live_metric_names(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .filter(|name| name.starts_with("im_"))
        .map(str::to_string)
        .collect()
}

fn metrics_operation_str(pointer: &str) -> String {
    spec_value()
        .pointer(pointer)
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!(
                "规范里 /metrics 的 GET operation 缺少 {pointer}。\
                 少了它, 测试 4 就退化成「什么都不检查」的空转断言。"
            )
        })
        .to_string()
}

/// 规范与代码之间的指标名差异。
///
/// 只 derive `Debug`: 失败信息里直接 `{:?}` 打两个字段。其余 derive 未用到,
/// 不写 —— 少一个 trait impl 少一处「以后有人以为它能用」的可能。
#[derive(Debug)]
struct MetricDrift {
    /// 规范 description 里写了, 但 `health.rs` 实际没暴露
    missing_in_code: Vec<String>,
    /// `health.rs` 实际暴露了, 但规范 description 里没写
    missing_in_spec: Vec<String>,
}

impl MetricDrift {
    fn is_clean(&self) -> bool {
        self.missing_in_code.is_empty() && self.missing_in_spec.is_empty()
    }
}

/// **双向**比较两份指标名清单, 返回两个方向的差异。
///
/// 为什么是双向而不是「文档 ⊆ 实际」: 单向漏掉「代码加了指标、规范没写」——
/// 而那恰好是集成方真正会踩的坑(规范是他们唯一的依据, 规范没写就等于不存在)。
///
/// 比较前两侧都排序, 故**顺序无关**: 调换 description 里指标的排列顺序不该
/// 让测试变红 —— 那是排版问题, 不是契约问题。
fn compare_metric_names(documented: &[String], exposed: &[String]) -> MetricDrift {
    let mut doc = documented.to_vec();
    let mut live = exposed.to_vec();
    doc.sort();
    live.sort();
    MetricDrift {
        missing_in_code: doc.iter().filter(|n| !live.contains(n)).cloned().collect(),
        missing_in_spec: live.iter().filter(|n| !doc.contains(n)).cloned().collect(),
    }
}

/// 反例守卫 —— 抽取规则本身必须经得起**故意构造的坏输入**。
///
/// 判别力的来源: 换一个极端输入, 结果**必须变**。
/// 若把上面两个 `fn` 的规则改坏(比如去掉大小写/字符集限制), 本用例会红;
/// 而如果它们退化成「返回全部反引号内容」或「恒返回空」, 本用例同样会红。
#[test]
fn metric_extraction_rejects_non_metrics_and_uppercase() {
    // 反例 1: 大写环境变量名不是指标(它在真实 description 里就存在)。
    assert!(
        backticked_metric_names("丢弃数因 `IM_EVENT_PUBLISHER_KIND=stub`").is_empty(),
        "`IM_EVENT_PUBLISHER_KIND` 是环境变量名, 绝不能被当成指标名"
    );

    // 反例 2: 含斜杠/大写/花括号的 URL 不是指标。
    let url_span = "客户端需经 `GET /v1/conversations/{id}/messages` 补齐";
    assert!(
        backticked_metric_names(url_span).is_empty(),
        "URL 跨度不是指标名"
    );

    // 反例 3: 没有反引号 = 没有可抽的东西(不能靠「看起来像」就放行)。
    assert!(
        backticked_metric_names("im_ws_broadcast_subscriptions 是 WS 连接数").is_empty(),
        "描述里没加反引号就该抽不到 —— 反引号是这个规则的前提, 不是装饰"
    );

    // 反例 4: 前缀对但混进非法字符, 仍然拒绝。
    assert!(
        backticked_metric_names("`im_bad-name` 与 `im_bad.name`").is_empty(),
        "含 `-` / `.` 的不是合法 Prometheus 指标名"
    );

    // 阳性对照: 真实形态必须抽得出(否则上面四条可能是因为「什么都抽不出」而绿)。
    let good = "`im_ws_indexed_conversations` (gauge) 与 `im_events_dropped_total` (counter)";
    assert_eq!(
        backticked_metric_names(good),
        vec![
            "im_ws_indexed_conversations".to_string(),
            "im_events_dropped_total".to_string()
        ],
        "合法指标名必须按出现顺序抽全"
    );

    // `live_metric_names` 的阳性对照: 注释行不得混进集合, 否则每个名字会出现 3 次。
    let body = "\
# MVP: prometheus exporter not yet enabled
# HELP im_ws_indexed_conversations 投递索引中登记的会话数
# TYPE im_ws_indexed_conversations gauge
im_ws_indexed_conversations 0
";
    assert_eq!(
        live_metric_names(body),
        vec!["im_ws_indexed_conversations".to_string()],
        "只应取数据行; # HELP / # TYPE / 顶部说明都必须跳过"
    );
}

/// `/metrics` 的 description 是一份**手写的散文**, 逐字复制 `health.rs` 里的
/// `# HELP` 文本。它没有任何东西守着 —— 于是每加一个指标就必然产生一次漂移:
/// 代码里有、规范里没有, 集成方按规范接进来就少一个可用指标, 而**没有任何
/// 测试会红**。这与「规范里写了路由但代码没注册」是同一类缺陷, 只是方向是
/// 散文 → 代码, 不在 `check-openapi.ps1`(静态比对路由表)也不在测试 1~3 的
/// 覆盖范围内。测试 4 就是补这个洞。
#[actix_web::test]
async fn metrics_description_matches_the_live_exposition() {
    let description = metrics_operation_str("/paths/~1metrics/get/description");
    let example =
        metrics_operation_str("/paths/~1metrics/get/responses/200/content/text~1plain/example");

    let app = actix_web::test::init_service(openapi_app()).await;
    let live = probe!(&app, "get", "/metrics");
    assert_eq!(
        live.status,
        StatusCode::OK,
        "/metrics 必须返回 200 —— 它是规范里 3 条根级 operation 之一。{}",
        live.describe()
    );
    let body = String::from_utf8_lossy(&live.body).into_owned();

    // --- 计数基线(阳性对照) -------------------------------------------------
    // 放在集合比较**之前**: 两个空集合是相等的, 若抽取逻辑坏掉导致两边都是空,
    // 下面的 assert_eq!(doc, live) 会**恒真通过**。先用一条独立基线把「确实抽到
    // 了东西」钉住, 集合比较才有意义。
    let live_names = live_metric_names(&body);
    assert_eq!(
        live_names.len(),
        EXPECTED_METRIC_COUNT,
        "health.rs 实际暴露的指标数从 {EXPECTED_METRIC_COUNT} 变成了 {}({live_names:?})。\
         有意增删时请同步改 EXPECTED_METRIC_COUNT, 并**同时**更新 /metrics 的 description, \
         否则下面那条双向比较会红。",
        live_names.len()
    );

    // --- 双向比较 -----------------------------------------------------------
    // 双向(set 相等)而不是单向(「文档 ⊆ 实际」): 单向漏掉「代码加了指标、
    // 规范没写」—— 而那恰好是集成方真正会踩的坑(规范是他们唯一的依据)。
    let drift = compare_metric_names(&backticked_metric_names(&description), &live_names);

    assert!(
        drift.is_clean(),
        "/metrics 的 description 与真实输出漂移了。\n\
         规范写了但代码没暴露: {:?}\n\
         代码暴露了但规范没写: {:?}\n\
         \n\
         两种修法: 改 `health.rs::metrics` 补齐指标, 或改 openapi.json 的 \
         `/metrics` description 去掉它。\n\
         真实输出:\n{body}",
        drift.missing_in_code,
        drift.missing_in_spec,
    );

    // --- example 的逐字核对 -------------------------------------------------
    // description 里的中文散文无法逐字机器校验(那是给人读的), 但 example 里的
    // `# HELP` 行是可以的 —— 逐行要求它**原样**出现在真实输出中。
    let absent = help_lines_absent_from(&example, &body);
    assert!(
        absent.is_empty(),
        "example 里的 `# HELP` 行与真实输出**逐字不符**: {absent:?}\n\
         它必须是 health.rs 里那一行的原样复制(不要改标点/空格/全半角)。\n\
         真实输出:\n{body}"
    );
}

/// 返回 example 里那些**没有原样**出现在真实输出中的 `# HELP` 行。
///
/// 「原样」= 整行字符串相等, 不是包含、不是去空白后相等。半角/全角括号、
/// 逗号后有没有空格都会被抓到 —— 这些正是「手抄一份 HELP 文本」最容易漂的
/// 地方, 而它们对读 `/metrics` 的人是可见的。
fn help_lines_absent_from(example: &str, body: &str) -> Vec<String> {
    example
        .lines()
        .filter(|l| l.starts_with("# HELP "))
        .filter(|line| !body.lines().any(|actual| actual == *line))
        .map(str::to_string)
        .collect()
}

/// example 里 `# HELP` 行的条数 —— 反例守卫要拿它当基线。
fn help_line_count(example: &str) -> usize {
    example.lines().filter(|l| l.starts_with("# HELP ")).count()
}

/// 变异守卫 —— 对**真实 spec 文本**做内存变异, 逐条证明测试 4 的判别力。
///
/// ## 为什么做成常驻测试, 而不是一次性脚本
///
/// 「门禁在故意注入缺陷时会红」这件事, 若只在我本机跑一次就扔掉, 下一个改动
/// 它照样能悄悄退化。留在这里, 它就是这条契约的**永久**反例守卫 —— 与
/// `undocumented_path_is_not_routed` 同一思路: 判别式本身必须被看守。
///
/// 变异全部作用在**从 `SPEC_JSON` 读出来的真实字符串**上, 不另造样本: 造样本
/// 只能证明「比较函数对假数据成立」, 而真实 spec 里那些干扰项(大写环境变量名、
/// 嵌在散文里的 URL)恰恰是最可能让抽取规则失手的地方。
#[actix_web::test]
async fn metrics_gate_rejects_mutated_specs() {
    let description = metrics_operation_str("/paths/~1metrics/get/description");
    let example =
        metrics_operation_str("/paths/~1metrics/get/responses/200/content/text~1plain/example");

    let app = actix_web::test::init_service(openapi_app()).await;
    let live = probe!(&app, "get", "/metrics");
    let body = String::from_utf8_lossy(&live.body).into_owned();
    let live_names = live_metric_names(&body);

    // --- 阳性对照: 未变异必须干净 -------------------------------------------
    // 若这一步就红, 说明下面的「变异被抓」没有意义(是常红的废物)。
    assert!(
        compare_metric_names(&backticked_metric_names(&description), &live_names).is_clean(),
        "未变异的真实 spec 与真实输出就已经漂移了 —— 变异用例的前提不成立"
    );
    assert_eq!(
        backticked_metric_names(&description).len(),
        EXPECTED_METRIC_COUNT,
        "对照组失效: 未变异时抽出的指标名就不是 {EXPECTED_METRIC_COUNT} 个"
    );
    assert!(
        help_line_count(&example) > 0,
        "对照组失效: example 里一条 `# HELP` 都没有, 逐字核对成了空转"
    );
    assert!(
        help_lines_absent_from(&example, &body).is_empty(),
        "对照组失效: 未变异的 example 已经与真实输出逐字不符"
    );

    // --- 变异 A: 规范少写一个指标(代码里有、规范没有) ----------------------
    // 这是集成方真正会踩的方向: 他按规范接, 规范没写的指标等于不存在。
    let dropped = "im_events_dropped_total";
    let mutated_a = description.replace(&format!("`{dropped}`"), "(该指标已下线)");
    assert_ne!(
        mutated_a, description,
        "变异 A 没生效: description 里找不到 `{dropped}`, 说明这条变异已过时, 请重写"
    );
    let drift_a = compare_metric_names(&backticked_metric_names(&mutated_a), &live_names);
    assert_eq!(
        drift_a.missing_in_spec,
        vec![dropped.to_string()],
        "变异 A 必须被抓, 且要指出「代码暴露了但规范没写」这一个名字"
    );

    // --- 变异 B: 规范多写一个不存在的指标 ----------------------------------
    let ghost = "im_ghost_metric";
    let mutated_b = format!("{description} 另有 `{ghost}` (gauge)");
    let drift_b = compare_metric_names(&backticked_metric_names(&mutated_b), &live_names);
    assert_eq!(
        drift_b.missing_in_code,
        vec![ghost.to_string()],
        "变异 B 必须被抓, 且要指出「规范写了但代码没暴露」这一个名字"
    );
    assert!(
        drift_b.missing_in_spec.is_empty(),
        "变异 B 只应在一个方向有差异, 另一方向不该被牵连"
    );

    // --- 变异 C: 改名 —— 两个方向同时被抓 ----------------------------------
    // 真实世界最常见的形态: health.rs 里把 `foo` 改名成 `bar`, 规范忘了跟。
    // 此时**不是**简单少一个, 而是「少 foo + 多 bar」, 只报一个方向会误导人。
    let mutated_c = description.replace(
        "`im_ws_indexed_conversations`",
        "`im_ws_conversations_indexed`",
    );
    let drift_c = compare_metric_names(&backticked_metric_names(&mutated_c), &live_names);
    assert_eq!(
        drift_c.missing_in_code,
        vec!["im_ws_conversations_indexed".to_string()],
        "变异 C: 改名后的新名字应报为 missing_in_code"
    );
    assert_eq!(
        drift_c.missing_in_spec,
        vec!["im_ws_indexed_conversations".to_string()],
        "变异 C: 被换掉的旧名字应报为 missing_in_spec"
    );

    // --- 变异 D: example 的 `# HELP` 改一个字 ------------------------------
    // 逐字核对这一路与上面三路独立: 名字集合完全没变, 只有描述文字漂了。
    // 少了它, 「名字对上了就算过」会让 HELP 文本永远无人看守。
    let mutated_d = example.replacen("已进入投递索引的 WS 连接数", "已进入索引的 WS 连接数", 1);
    assert_ne!(
        mutated_d, example,
        "变异 D 没生效: example 里找不到那段 HELP 文本, 请重写这条变异"
    );
    assert!(
        !help_lines_absent_from(&mutated_d, &body).is_empty(),
        "变异 D 必须被抓: 只改一个字而指标名集合不变, 正是逐字核对存在的意义"
    );
    // 反向自检: 逐字核对不能「什么都判不合」—— 未变异时必须是干净的。
    assert!(
        help_lines_absent_from(&example, &body).is_empty(),
        "反例守卫: 逐字核对在未变异时误报, 说明它恒红"
    );

    // --- 反例: 纯排版变化**不得**被抓 --------------------------------------
    // 调整 description 里指标的排列顺序, 以及去掉多余空格, 都不改变契约。
    // 若这些让测试变红, 开发者就会开始绕过测试 —— 那比漏检更糟。
    let reordered = description
        .replace(" (gauge)", "")
        .replace(" (counter)", "");
    assert!(
        compare_metric_names(&backticked_metric_names(&reordered), &live_names).is_clean(),
        "去掉 (gauge)/(counter) 标注不应被判成漂移 —— 指标名集合没变"
    );
}
