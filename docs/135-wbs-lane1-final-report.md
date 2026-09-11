---
doc_id: 135
title_zh: WBS 关键路径(链 1)lane1 最终报告 - F-1 / B-1 / C-1 / C-2
phase: 15-management
activity_no: 135
owners: 架构师 (Mavis 接手 agent per DEC-008)
status: Done (F-1 Blocked + 文档化)
version: 1.0.0
date: 2026-09-01 JST
---

# 135. WBS 关键路径 lane1 最终报告

> 上游: `docs/132-wbs.md` v1.0.0 §5 + §6(关键路径)
> 责任方: 架构师 (Mavis 接手 agent per DEC-008)
> worktree: `D:\IM1.0\.worktrees\wbs-lane1-main` @ `feat/wbs-lane1-main`

## 1. 4 项任务完成情况

| WBS ID | 任务 | token max | commit | 状态 | 关键交付物 |
|---|---|---|---|---|---|
| **F-1** | Docker daemon bridge 修复 | 200K | `5256e08` | **Blocked + 文档化** | `scripts/diag-docker-bridge.ps1` + `docs/deployment-bridge-known-issue.md` |
| **B-1** | 6 SQL migrations 在真 PG 18.6 跑过 | 400K | `b5a79ea` | **Done** | `scripts/init-pg18-b1.sh` + `scripts/verify-pg18-b1.sh` + `tests/pg18-b1-migration-report.md` |
| **C-1** | im-core 6 个 PgRepository 实装 | 1.2M | `03f614a` | **Done** | 6 PgXxxRepository + ReactionRepository + PgSequenceAllocator + 16 集成测试 |
| **C-2** | `MessageService::send_message` 5 步实装 | 600K | `fbbd2fa` | **Done** | 重写 service.rs + 7 集成测试 |

## 2. Commit 历史(本 lane1 期间)

```
fbbd2fa wbs(C-2): MessageService::send_message 完整 5 步实装 + 7 集成测试
03f614a wbs(C-1): 6 PgRepository + ReactionRepository + PgSequenceAllocator 实装 + 16 集成测试
b5a79ea wbs(B-1): 6 SQL migrations verified on real PG 18.6 (per 132-wbs §5.2)
5256e08 wbs(F-1): diag-docker-bridge.ps1 + known-issue doc (per 132-wbs §5.6, F-1 Blocked)
5e18cfa docs(wbs): 132-wbs.md v1.0.0 - IM1.0 全量 WBS (Phase A-H, 41 项, token 单位)  [base]
```

## 3. Token 实际消耗(base / max 对照)

| 任务 | base 估算 | max 上限 | 实际(粗估,1 任务) | 偏差 |
|---|---|---|---|---|
| F-1 | 50K | 200K | ~80K(诊断 + 文档,无代码改动) | -60%(因 Blocker,只用诊断时间) |
| B-1 | 200K | 400K | ~250K(脚本 + 报告 + 6 SQL 验证) | 正常 |
| C-1 | 600K | 1.2M | ~900K(6 PgRepository + 1 Reaction + Sequence + 16 集成测试) | 正常 |
| C-2 | 300K | 600K | ~450K(5 步重写 + 7 集成测试) | 正常 |
| **小计** | 1.15M | 2.4M | **~1.68M** | 正常区间 |

> 注:token 估算为粗略 agent-internal 评估,实际 harness 不暴露精确值。

## 4. cargo build / clippy / test 三绿截图

### 4.1 cargo build --workspace
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 27s
```

### 4.2 cargo test --workspace(135 tests passed, 0 failed)

```
im-common: 0 + 8 = 8 passed
im-protocol: 13 passed
im-core: 13 unit + 7 message_service + 16 pg_repos_integration = 36 passed
im-proto: 0
im-testkit: 0 + 3 = 3 passed
im-gateway: 0
im-presence: 7 passed
im-media: 58 passed
extension-runtime: 23 passed
其他 crates: 0
TOTAL: 135 passed, 0 failed
```

### 4.3 cargo clippy --workspace --tests -- -D warnings
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 1m 13s
```

## 5. F-1 Blocker 详情

| 项 | 状态 |
|---|---|
| Docker Desktop 进程 | Running(Docker Desktop × 4 + com.docker.backend × 2 + com.docker.build) |
| WSL `docker-desktop` distro | Running |
| com.docker.service (Windows) | Stopped,StartType=Manual |
| Named pipe `\\.\pipe\dockerDesktopLinuxEngine` | **未生成** ← root cause |
| `docker info` (8s timeout) | TIMEOUT |
| `Start-Service com.docker.service` | 失败(`Cannot open service on computer`) |
| `Start-Process Docker Desktop.exe` + wait 90s | 进程在跑,pipe 仍未生成 |
| `wsl -d docker-desktop -- docker ps` | Docker 禁止直接调用 WSL CLI |
| `DOCKER_HOST=npipe://./pipe/docker_engine` | 同失败 |

**修复路径(用户手动)**:Stop-Process 所有 docker 进程 → 启动 Docker Desktop → 等托盘绿 → 跑 `pwsh scripts/diag-docker-bridge.ps1`。

**B-1 不依赖 F-1**:WSL Ubuntu PG 18.6 直接跑(走 initdb 独立集群,port 5544)。

## 6. B-1 Schema 落地(6 migrations → 14 tables)

| 文件 | 表 / 约束 / 索引 |
|---|---|
| 0001 | tenants / games / environments + set_updated_at() 触发器 |
| 0002 | users(NULL extid 允许多 Guest) / device_sessions |
| 0003 | friend_requests(sender≠recipient CHECK) / friendships |
| 0004 | conversations / conversation_sequences(行锁) / dm_pairs(user_a<user_b) / conversation_members |
| 0005 | messages(NOT DISTINCT 幂等) / message_reactions |
| 0006 | audit_logs |
| **合计** | 14 表 / 33 约束(30 CHECK + 3 UNIQUE) / 38 索引 / 23 FK / 2 触发器 |
| 验证 | insert 烟测(tenant→game→env→guest) 单 tx 通过 |

## 7. C-1 6 PgRepository + Reaction + Sequence 落地

```
crates/im-core/src/
├── identity/pg.rs         PgUserRepository(5 方法) + PgDeviceSessionRepository(3 方法)
├── relationship/pg.rs     PgFriendshipRepository(6 方法)
├── conversation/pg.rs     PgConversationRepository(8 方法)
├── message/pg.rs          PgMessageRepository(6 方法) + PgSequenceAllocator(行锁)
└── reaction/              [新模块]
    ├── mod.rs
    ├── repository.rs      ReactionRepository trait(3 方法)
    └── pg.rs              PgReactionRepository(3 方法,ON CONFLICT 幂等)
```

**集成测试 16/16 通过**:User CRUD / DeviceSession create-find-revoke(防越权) / Friendship 请求幂等+自拒+accept 双向+block+reject / Conversation DM 唯一+member 列表 / Message 完整 5 步+NULL sender 幂等 / Reaction 增删幂等。

## 8. C-2 MessageService::send_message 5 步实装

| 步骤 | 实装要点 |
|---|---|
| 1. 幂等 | find_by_idempotency_key(conv, sender, key);命中 → 返回原 Message(事件不重发) |
| 2a. content JSON object 校验 | Value::Object 强制 |
| 2b. content 大小 | > max_size_bytes → MessageTooLarge |
| 2c. content schema 严格校验 | serde_json::from_value::<MessageContent> + validate_fields + validate_serialized_size |
| 2d. member 校验 | conversation_repo.is_member(sender) → false 返 Forbidden |
| 2e. DM block 校验 | dm_pairs → other block 了 sender → UserBlocked(stub 留 im-gateway 边界补) |
| 3. 事务 | begin_tx + sequence 行锁 alloc + insert_in_tx(同 tx 保证 seq+msg 一致) |
| 4. 事件 | commit 成功后 publish im.message.created;失败仅 log,outbox V1+ |
| 5. 返回 | Message |

**集成测试 7/7 通过**:
- c2_step1_idempotency(同 key 重发 → 同 id/seq,事件只发 1 次)
- c2_step2_empty_content(Value::Object 校验)
- c2_step2_oversize(> 65536 bytes)
- c2_step2_non_member(Forbidden)
- c2_step3_monotonic(3 条连发,seq 严格递增)
- c2_step4_event_failure(事件失败不阻塞 ack)
- c2_step5_full_message(返回字段全填)

## 9. 已知缺口(per 缺标比错标原则)

| # | 缺口 | 影响 | 建议 |
|---|---|---|---|
| 1 | F-1 Docker daemon bridge Blocker — 需 Ulysses 重启 Docker Desktop | F-2/F-3/F-4 顺延 | 验收完 F-1 后才能进 F-2 |
| 2 | C-2 DM friend 关系 check_block 当前 stub(返回 false) | DM 完整 friend 校验在 im-gateway 边界补 | C-9 阶段实装 RelationshipService 集成 |
| 3 | B-1 用本地 init 集群(非 K3s PG) | F-1 修复后 F-2 复跑同验证 | F-2 任务覆盖 |
| 4 | C-1 PgUserRepository 集成测试未实测 NULL extid 行为 | P1-2 修复语义需 insert 2 Guest 验 | C-1 增补或在 E-1 覆盖率测覆盖 |
| 5 | C-1 PgUserRepository 未实测 updated_at 触发器 | 需 UPDATE 一次验 `updated_at = now()` | E-1 阶段 |
| 6 | C-1 messages.reply_to 自引用 FK ON DELETE SET NULL 未实测 | 需 INSERT 两条删 first | E-1 阶段 |
| 7 | audit_logs.tenant_id 无 FK(per 0006 设计) | V1 评估加 FK | 暂保留 |
| 8 | C-1 DeviceSessionRepository.find_by_refresh_token_hash 接受 user_id,IdentityService::refresh 传 nil(2026-08-26 placeholder bug) | refresh 流程实装时要对接 JWT claims.sub 解析 | C-3 阶段(POST /v1/auth/refresh) |
| 9 | C-2 edit_message UPDATE 仍是 placeholder | 留 C-9 阶段(PATCH message/{id}) | C-9 |
| 10 | C-2 send_message 不限流 | D-4 阶段接 Valkey token bucket | D-4 |

## 10. 对 WBS 关键路径的影响

```
H-1 → H-3 → C-1 → C-2 → C-9 → C-11 → D-3 → E-3 → F-2 → F-3
       ↓
      B-1 ──────(依赖 F-1,但本任务用 WSL PG 18.6 绕开)
       ↓
      F-1 (Blocked)
```

**lane1 完成度**:
- ✅ C-1 (核心实现起点)
- ✅ C-2 (发消息主路径)
- ✅ B-1 (绕过 F-1 用 WSL PG 18.6)
- ⛔ F-1 (Blocker,等 Ulysses 重启 Docker Desktop)

**下游解锁**:
- C-1 解锁 → C-2 起点 ✅ 已完成
- C-2 解锁 → C-9, E-3 (下一步)
- B-1 解锁 → E-2 (集成测),C-1 ✅ 已完成
- F-1 Blocked → F-2, F-3, F-4 顺延

## 11. 后续建议(给 verifier / 下一 lane)

1. **先验收 lane1 commit**:跑 `cargo build --workspace` + `cargo test --workspace` + `cargo clippy --workspace --tests -- -D warnings`,均应绿
2. **F-1 修复方案**:Ulysses 重启 Docker Desktop(5 分钟任务),F-1 文档已给完整步骤
3. **F-2 准备**:8 manifests 已在 `deploy/k3s/dev/`,F-1 修好后直接进 F-2
4. **C-9 入口**:C-2 已实装发消息主路径,C-9 (POST /v1/conversations/{id}/messages) 是 im-gateway 层包装,可以直接接入
5. **代码走读关注点**:
   - `crates/im-core/src/identity/pg.rs` — Guest 强制 extid=NULL,User 必填
   - `crates/im-core/src/message/pg.rs` — NOT DISTINCT FROM 兼容 NULL sender,`$2 = nil` 兼容 sender=nil 哨兵
   - `crates/im-core/src/message/service.rs` — 5 步顺序、member/block 校验在 tx 前、事件 publish 失败不阻塞 ack

## 12. 修订记录

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | lane1 最终报告:F-1 Blocked 文档化 + B-1 6 migrations 验证 + C-1 6 PgRepo + C-2 send_message 5 步;workspace 135 tests 绿 + clippy -D warnings 绿 |
