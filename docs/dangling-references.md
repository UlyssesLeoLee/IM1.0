# 悬空引用清单 (Dangling References)

> **本文档是一份事实记录,不是规范** —— 它只记录「代码里引用了什么」以及
> 「被引用的东西是否存在」。**本文档不定义 守门 #N 的含义。**
>
> 关联文档:
> - `Project-Status.md` —— 项目当前状态快照
> - `138-dev-plan.md` —— 守门编号出现最密集的文档

---

## 0. 结论摘要 (Summary)

仓库内多处源码注释引用 `守门 #N` 形式的规则编号,但**本仓库不存在任何定义这些
编号的文件**。已验证(2026-10-02,dev @ `803bad9`):

| 待查文件 | 仓库根是否存在 | 验证方式 |
|---|---|---|
| `AGENTS.md` | ❌ 不存在 | 全仓 glob `**/AGENTS.md` → 无匹配 |
| `CLAUDE.md` | ❌ 不存在 | 仓库根文件列举 → 无 |
| `rustfmt.toml` | ❌ 不存在 | 仓库根文件列举 → 无 |

因此,当前所有 `守门 #N` 引用**均无法在本仓库内解析**。

代码中的 `守门 #N` 引用共 **46 处**,分布在 **13 个文件**、**46 行**,
涉及 **7 个不同编号**(`#1` / `#6` / `#7` / `#9` / `#11` / `#14` / `#15`)。

> **统计口径**:按「编号出现次数」计,正则 `守[门門]\s*#(\d+)`(兼容简体/繁体
> 「守門」两种写法),扫描 `Cargo.toml` + `crates/` + `docs/` + `scripts/` +
> `tests/` + `config/` + `deploy/` + `docker/` + `migrations/` 下的
> `.rs/.toml/.md/.py/.sh/.ps1/.yml/.json`。**本文件自身不计入**(否则自我引用
> 会污染计数)。

---

## 1. 编号清单与出现次数 (Inventory)

| 编号 | 出现次数 | 引用处的文字线索 (原文摘录) |
|---|---|---|
| `#1` | 20 | 「已知缺口 (per 守门 #1 缺标比错标)」「缺口台账: 缺口 #A..#I」 |
| `#6` | 4 | 「派工策略 (per守门 #6 lane1..6 模式)」 |
| `#7` | 7 | 「守门 #7 max 2 retries 触发」 |
| `#9` | 4 | 「跨语言 dispatch (per 守门 #9)」 |
| `#11` | 3 | 「守门 #11 缺标比错标: git dep 锁 rev=...」 |
| `#14` | 7 | 「代签规则 (守门 #14 v3+v4)」 |
| `#15` | 1 | 「per 守門 #15 v3」 |
| **合计** | **46** | 46 行 / 13 文件 |

> **编号离散度**:`#1` 遍布 `crates/im-gateway/` 多个文件(20 处,占总数 43%),
> 是目前引用最密集的编号;`#7` / `#14` 几乎全部集中在 `docs/138-dev-plan.md`
> 一个文件内;`#9` 分布在 im-testkit 的 4 个文件。这说明编号体系**不存在
> 单一权威来源**,而是由各次会话各自引用、就地解释。
>
> **变化记录**:`#1` 的引用数在 2026-10-02 的 clippy 清零工作中从 5 增至 20 ——
> im-gateway 的 12 处死代码改为「标注保留」时,每处都补了
> `守门 #1 缺口台账: 缺口 #X` 注释(缺口编号 #A..#I)。这是引用数上升的唯一原因,
> 属有意为之(把原本无标注的 WIP 登记进台账)。

---

## 2. 示例位置 (Example Locations)

行号对应 dev @ `803bad9`。

| 文件:行 | 原文摘录 |
|---|---|
| `Cargo.toml:147` | `# 守门 #11 缺标比错标: git dep 锁 rev=df28c56 (= ULYS-191.1 / ULYS-224 main HEAD)` |
| `crates/im-testkit/src/lib.rs:44` | `//! subprocess 暴露 (per AGENTS.md 守门 #9)。` |
| `crates/im-testkit/tests/im_testkit_module_switch.rs:8` | `//! These tests use subprocess to invoke the Python helper (per 守门 #9) so the` |
| `crates/im-testkit/scripts/_lib_mock_switch_im.py:15` | `跨语言调用: 通过 subprocess 调 (per 守门 #9); Rust native 版本跨 session (G-MS-BRIEF-S44-01).` |
| `crates/im-testkit/docs/aci-integration.md:30` | `# 守门 #11 缺标比错标: git dep 锁 rev=df28c56 ...` |
| `crates/im-gateway/src/ws/handler.rs:19` | `//! ### 已知缺口 (per 守门 #1 缺标比错标)` |
| `crates/im-gateway/src/main.rs:54` | `// 守门 #1 缺口台账: 缺口 #A — http/auth_handlers.rs 响应 expires_in 当前写死 900 ...` |
| `crates/im-gateway/src/http/error_response.rs:26` | `// 守门 #1 缺口台账: 缺口 #B — 现有 handler 走 crate::error::error_to_response ...` |
| `crates/im-gateway/src/http/auth_handlers.rs:202` | `// 守门 #1 缺口台账: 缺口 #C — task 规范要求的可选 fingerprint ...` |
| `crates/im-gateway/src/http/auth_handlers.rs:408` | `// 兜底: 返 501 Not Implemented + 缺口描述 (per守门 #1 缺标比错标)` |
| `crates/im-gateway/src/ws/heartbeat.rs:27` | `// 守门 #1 缺口台账: 缺口 #D — C-11 driver 接线后用于检测 ping 迟到` |
| `crates/im-gateway/src/ws/session.rs:95` | `// 守门 #1 缺口台账: 缺口 #G — 鉴权后身份读取接口,待 C-11 driver 做 ForceDisconnect` |
| `docs/138-dev-plan.md:128` | `cargo test -p im-gateway`: ... **守门 #7 max 2 retries 触发 ...` |
| `docs/138-dev-plan.md:289` | `**派工策略** (per守门 #14 v3+v4):` |
| `docs/138-dev-plan.md:296` | `- 任一 worker 完成 → 立即 merge + cleanup worktree (per守门 #6 lane1..6 模式)` |

---

## 3. 显式点名 `AGENTS.md` 的引用 (Explicitly Named)

有 **4 处**直接写出 `AGENTS.md` 文件名,而该文件不存在:

| 文件:行 | 原文摘录 | 引用形式 |
|---|---|---|
| `crates/im-testkit/src/lib.rs:44` | `//! subprocess 暴露 (per AGENTS.md 守门 #9)。` | 章节 + 编号 |
| `crates/im-testkit/tests/aci_integration.rs:11` | `//! 守门 (per AGENTS.md §4):` | 仅章节 |
| `crates/im-testkit/docs/regression-report-stage1-module-switch-2026-09-25.md:119` | `## 5. 守门合规 (per AGENTS.md §4)` | 仅章节 |
| `crates/im-testkit/docs/regression-report-stage1-module-switch-2026-09-25.md:160` | `8. 🟡 **Layer 1→Layer 2→Layer 3 贯通验收** (per AGENTS.md §3, 顶层 sub-task)` | 仅章节 |

这 4 处是**具名悬空引用** —— 至少指明了「定义在哪」,只是目标文件缺失。
其余 **42 处**只写「守门 #N」而不指向任何文件,属**匿名悬空引用**
(连定义所在的文件名都没有,更难追溯)。

值得注意的是,`AGENTS.md` 被以**两种不同的引用粒度**使用:
「`§3` / `§4` 章节号」与「`守门 #9` 规则编号」。这两套编号空间是否同源、
是否需要同时保留,**无法从本仓库判定**,须由团队 lead 澄清。

---

## 4. 明确不做的事 (Out of Scope — 禁止猜测)

以下动作**均未执行**,且在得到团队 lead 提供的原始定义前**不应执行**:

- ❌ **不新建 `AGENTS.md`** —— 凭空造一份等于伪造规范,比悬空引用更有害。
- ❌ **不推测 `守门 #1..#15` 的含义** —— 现有文字线索(如 `#1` 旁注
  「缺标比错标」、`#7` 旁注「max 2 retries」、`#14` 旁注「v3+v4」)
  只是**各次会话的局部用法**,不构成编号的正式定义。- ❌ **不根据 `#1..#15` 的编号连续性推断「还有 #2/#3/#4…」** ——
  编号有跳空,且 `#15` 只出现 1 次,不足以判断编号空间。
- ❌ **不重写源码注释以移除这些引用** —— 移除会丢失「此处曾受某规则约束」
  这一真实信息,且需先知道规则内容才能决定如何替换。

---

## 4.1 已消解的悬空引用 (2026-10-03 更新)

本清单自建立后已消解两处**具名**悬空引用。留档以说明消解方式与边界:

| 原引用 | 问题 | 消解方式 |
|---|---|---|
| `crates/im-gateway/tests/migration_smoke_docker.rs` | 文档称"完整版见此文件 (feature-gated)",**该文件从不存在** | 改为指向真实新增的 `migration_smoke_pg.rs`;并在 `migration_smoke.rs` 里注明更正 |
| 「守门 #1 缺口台账」(26 处代码注释) | 代码引用一份**不存在的台账文档** | 新增 `docs/gap-ledger.md`,**只汇总代码注释里已有的声明**, 逐条附源码位置与接线条件 |

**两处消解的共同边界**: 都只**汇集/更正在别处已存在的事实**, 没有为
`守门 #N` 编造任何定义。`守门 #1` 本身在本仓库**仍无定义文档**, 需要团队
lead 提供原始定义才能消解 —— 这一点没有变。

## 5. 需要团队 lead 提供的材料 (What to Ask the Team Lead)

要消解这 46 处悬空引用,需要以下**事实性**输入,任一形式均可:

1. `守门 #1..#N` 编号体系的**原始定义文档**(可能在 Star 仓库或
   `docs/138-dev-plan.md` 的上游工作区,本仓库未收录);
2. 或直接给出每个被引用编号(`#1` `#6` `#7` `#9` `#11` `#14` `#15`)
   的一句话释义 + 版本号(注释中出现 `v3` / `v4` / `v15` / `v25` 等
   版本后缀,说明该体系本身有版本演进)。

拿到材料后,应在**权威文档**中建立定义,再逐处把 `per AGENTS.md 守门 #N`
改为指向该权威文档的相对路径。

---

**维护**:本清单为快照,记录于 dev @ `803bad9`(2026-10-02)。
若后续新增 `守门 #N` 引用或补入定义文档,需重新统计并更新本表。
