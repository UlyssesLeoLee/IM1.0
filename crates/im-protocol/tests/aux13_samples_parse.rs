//! aux-13 §1.1 / §1.2 的 **JSON 样例**必须能被真实 serde 类型解析
//!
//! ## 为什么需要这个测试
//!
//! 2026-10-07 实测: 规范所有者拍板给 `message_edited` / `reaction_added` 补
//! `conversation_id` 并实装广播后, **aux-13 里的样例 JSON 仍是旧形状**
//! (`message_edited` 3 字段、`reaction_added` 4 字段)。
//!
//! 后果很具体: 接入方照抄样例写解析分支, 收到真实下行的帧会**解析失败**
//! (缺必填字段)。而当时仓里**没有任何东西**能发现这件事 ——
//! `check-asyncapi.ps1` 比对的是 `docs/api/asyncapi.json` ↔ serde, 不碰 aux-13;
//! `ws_frames_contract.rs` 用的是**手写** JSON, 不是文档里的样例。
//!
//! 也就是说「文档与代码一致」这件事当时**无人看守**, 于是它就不一致了。
//!
//! ## 判据
//!
//! 逐个 §1.1.x / §1.2.x 小节取出 ```json 块, 按方向解析:
//!   - `#### 1.1.x` → [`ClientFrame`]   (客户端 → 服务端)
//!   - `#### 1.2.x` → [`ServerFrame`]   (服务端 → 客户端)
//!
//! 注意 `#[serde(tag = "type")]` 的必填字段是**真的会拒**的:
//! `ServerFrame::MessageEdited::conversation_id` 是非 `Option` 的 `Uuid`, 且
//! 无 `default`, 所以旧样例必然解析失败 —— 这正是本测试要抓的东西。
//!
//! ## 反例守卫(fail-closed)
//!
//! 样例数量有下限断言: 把样例整段删掉, "没有样例可解析" 会让本测试**空跑通过**,
//! 那比没有测试更坏。同理, 若某小节没有 ```json 块, 记为失败而不是跳过 ——
//! 「取不到」必须响, 不能当成「没问题」。
//!
//! 用法: cargo test -p im-protocol --test aux13_samples_parse

use std::fs;
use std::path::PathBuf;

use im_protocol::ws_frames::{ClientFrame, ServerFrame};

/// aux-13 §1.1(客户端 8 帧)与 §1.2(服务端 12 小节)的样例条数下限。
///
/// 取自 2026-10-07 的实际值(§1.1 8 条 + §1.2 12 条 = 20)。写成下限而非精确值,
/// 是为了允许规范**新增**小节而不必改本测试; 但删掉样例会让本测试变红。
const MIN_CLIENT_SAMPLES: usize = 8;
const MIN_SERVER_SAMPLES: usize = 12;

fn aux13_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("docs")
        .join("templates")
        .join("04-detailed-design")
        .join("auxiliary")
        .join("aux-13-protocol-frame-samples.md")
}

/// 取出 `#### 1.1.x` / `#### 1.2.x` 小节及其 ```json 块。
///
/// 返回 `(小节号, 该小节的第一个 json 块)`。找不到 json 块时后者为 `None`
/// —— 由调用方记为失败, **不**静默跳过。
fn extract_samples(doc: &str) -> Vec<(String, Option<String>)> {
    // 先按行扫出每个 `#### 1.x.y` 标题的位置, 再按位置切片
    let mut heads: Vec<(usize, String)> = Vec::new();
    let mut offset = 0usize;
    for line in doc.split_inclusive('\n') {
        let trimmed = line.trim_start_matches('#').trim();
        if let Some(rest) = trimmed.strip_prefix("1.") {
            // 形如 `1.1.1 \`auth\`` 或 `1.2.6 \`message_edited\``
            let num: String = rest
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            let parts: Vec<&str> = num.trim_end_matches('.').split('.').collect();
            if parts.len() == 2
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            {
                heads.push((offset, format!("1.{}.{}", parts[0], parts[1])));
            }
        }
        offset += line.len();
    }

    let mut out = Vec::new();
    for (i, (pos, name)) in heads.iter().enumerate() {
        let end = heads.get(i + 1).map(|(p, _)| *p).unwrap_or(doc.len());
        let body = &doc[*pos..end];

        // 只认围栏式 ```json 块; 块内不得再出现 ``` (样例里不会有, 但万一有)
        let mut json_block: Option<String> = None;
        let mut inside = false;
        let mut buf = String::new();
        for line in body.split_inclusive('\n') {
            let t = line.trim_end();
            if !inside && t.starts_with("```json") {
                inside = true;
                buf.clear();
                continue;
            }
            if inside {
                if t.starts_with("```") {
                    // 不写 `inside = false`: 紧接着就 break, 这个赋值永远不会被
                    // 读到, 而 clippy -D warnings 会把它当错误(本仓 CI 就是
                    // `clippy -- -D warnings`)。行为完全一致。
                    json_block = Some(buf.clone());
                    break;
                }
                buf.push_str(line);
            }
        }
        out.push((name.clone(), json_block));
    }
    out
}

#[test]
fn aux13_samples_parse_into_real_serde_types() {
    let path = aux13_path();
    let doc =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}: {e}", path.display()));

    let samples = extract_samples(&doc);

    // ---- 反例守卫: 样例被整段删掉时必须红, 不能空跑通过 ----
    let client_count = samples
        .iter()
        .filter(|(n, _)| n.starts_with("1.1."))
        .count();
    let server_count = samples
        .iter()
        .filter(|(n, _)| n.starts_with("1.2."))
        .count();
    assert!(
        client_count >= MIN_CLIENT_SAMPLES,
        "aux-13 §1.1 样例只找到 {client_count} 个, 少于下限 {MIN_CLIENT_SAMPLES} —— \
         是样例被删了, 还是本测试的抽取逻辑坏了? 两种都必须查清, 不能当成通过。"
    );
    assert!(
        server_count >= MIN_SERVER_SAMPLES,
        "aux-13 §1.2 样例只找到 {server_count} 个, 少于下限 {MIN_SERVER_SAMPLES} —— \
         是样例被删了, 还是本测试的抽取逻辑坏了?"
    );

    // ---- 逐条解析; 收集**全部**失败后一次报出, 而不是遇到第一条就返回 ----
    let mut failures: Vec<String> = Vec::new();
    let mut parsed = 0usize;

    for (name, block) in &samples {
        let Some(json) = block else {
            failures.push(format!("{name}: 小节里没有 ```json 样例块"));
            continue;
        };
        match name.split('.').nth(1) {
            Some("1") => match serde_json::from_str::<ClientFrame>(json) {
                Ok(_) => parsed += 1,
                Err(e) => failures.push(format!(
                    "{name}: 解析为 ClientFrame 失败: {e}\n样例:\n{json}"
                )),
            },
            Some("2") => match serde_json::from_str::<ServerFrame>(json) {
                Ok(_) => parsed += 1,
                Err(e) => failures.push(format!(
                    "{name}: 解析为 ServerFrame 失败: {e}\n样例:\n{json}"
                )),
            },
            other => failures.push(format!("{name}: 意料之外的小节号 {other:?}")),
        }
    }

    assert!(
        failures.is_empty(),
        "aux-13 的样例有 {}/{} 条**解析不了**。\n\
         接入方照抄样例写的解析分支会因此失败 —— 样例必须与代码一致。\n\n{}\n\n\
         修法: 要么按代码事实更新 aux-13 样例, 要么(若确属规范未同步)先取得规范所有者裁决。\n\
         **不要**在本测试里加例外名单 —— 每一条例外都是一个接入方会踩的坑。\n",
        failures.len(),
        samples.len(),
        failures.join("\n---\n")
    );

    assert!(parsed >= MIN_CLIENT_SAMPLES + MIN_SERVER_SAMPLES);
}

/// 方向别搞反: §1.2 是**服务端**下行帧。若把 ServerFrame 当 ClientFrame 解析,
/// 上面那条会**整体空跑**(每条都失败但断言只看 failures.is_empty, 仍会红) ——
/// 所以本测试专门钉住两件事, 防止将来有人把两个方向写反。
#[test]
fn section_direction_is_pinned() {
    let path = aux13_path();
    let doc =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("读不到 {}: {e}", path.display()));
    let samples = extract_samples(&doc);

    let client = samples
        .iter()
        .find(|(n, _)| n == "1.1.2")
        .expect("应存在 §1.1.2 send_message");
    let server = samples
        .iter()
        .find(|(n, _)| n == "1.2.5")
        .expect("应存在 §1.2.5 message_new");

    let c = client.1.as_deref().expect("§1.1.2 应有 json 块");
    let s = server.1.as_deref().expect("§1.2.5 应有 json 块");

    // 交叉解析: §1.1.2 的样例**不该**能解析成 ServerFrame
    assert!(
        serde_json::from_str::<ServerFrame>(c).is_err(),
        "§1.1.2 是客户端上行帧, 不该解析成 ServerFrame —— 若能, 说明方向反了"
    );
    // §1.2.5 的样例**不该**能解析成 ClientFrame
    assert!(
        serde_json::from_str::<ClientFrame>(s).is_err(),
        "§1.2.5 是服务端下行帧, 不该解析成 ClientFrame —— 若能, 说明方向反了"
    );
}
