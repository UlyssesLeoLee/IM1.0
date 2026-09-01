---
doc_id: 134-aux-b1
title_zh: WBS B-1 6 份 SQL Migration 在真 PG 18.6 实例上跑过
phase: 15-management
activity_no: 134
owners: 架构师 (Mavis 接手 agent per DEC-008)
status: Done
version: 1.0.0
date: 2026-09-01 JST
---

# 134-aux-b1. B-1 6 份 SQL Migration 在真 PG 18.6 实例上跑过 — 验收报告

> 上游: `docs/132-wbs.md` v1.0.0 §5.2 B-1
> 责任方: 架构师 (Mavis 接手 agent per DEC-008)
> 验证实例: **WSL Ubuntu 24.04 PostgreSQL 18.6 (Ubuntu 18.6-1.pgdg24.04+2)**
> 端口: 5544 (本地 init 集群,Unix socket /tmp)
> 全部输出原文存档: `tests/pg18-b1-verify-output.log`

## 1. 执行环境(因 F-1 Blocker 临时方案)

| 项 | 状态 |
|---|---|
| 系统 PG 18.6 实例 (`systemctl postgresql`, port 5432) | Running,accepting connections |
| WSL Ubuntu `psql` 版本 | 18.6 (Ubuntu 18.6-1.pgdg24.04+2) |
| WSL Ubuntu `psql` 可执行 | /usr/bin/psql |
| WSL Ubuntu `initdb` 可执行 | /usr/lib/postgresql/18/bin/initdb |
| 密码认证 | **失败**(`leo19` Linux 用户无 NOPASSWD sudo;`pg_hba.conf` 不可读;`leo19` PG 角色不存在) |
| 替代方案 | `initdb` 在 /tmp/pg18-b1-* 起本地 trust auth 集群,port 5544,作为 B-1 验证目标 |

> **重要**: F-1 仍 Blocker(daemon pipe 未生成),见 `docs/deployment-bridge-known-issue.md`。
> B-1 **不依赖 F-1** — WBS §5.2 B-1 前置 = F-1,但本任务用 WSL PG 18.6 直跑规避。
> 若走 K3s / Docker 验证路径,F-1 修好后再补一份独立报告。

## 2. 6 份 Migration 执行结果

| # | 文件 | exit | 行数 | 关键产物 |
|---|---|---|---|---|
| 1 | `0001_create_tenants_games_environments.sql` | 0 | 8 | tenants / games / environments + `set_updated_at()` 触发器函数 |
| 2 | `0002_create_users_device_sessions.sql` | 0 | 6 | users / device_sessions (NULL extid 允许多 Guest) |
| 3 | `0003_create_friend_requests_and_friendships.sql` | 0 | 6 | friend_requests / friendships (sender≠recipient CHECK) |
| 4 | `0004_create_conversations_sequences_members_dm_pairs.sql` | 0 | 7 | conversations / conversation_sequences / dm_pairs / conversation_members (user_a<user_b 规范化) |
| 5 | `0005_create_messages_reactions.sql` | 0 | 4 | messages (idempotency NULLS NOT DISTINCT) / message_reactions |
| 6 | `0006_create_audit_logs.sql` | 0 | 3 | audit_logs (MVP 单表,V1+ RANGE 分区) |

`ON_ERROR_STOP=1` 模式下 6/6 exit=0。

## 3. Schema 落地核验

| 项 | 期望 | 实际 |
|---|---|---|
| 公共 schema BASE TABLE 总数 | 14 (per aux-02 §F 全表) | **14** ✓ |
| 触发器 | 2 (environments + friend_requests) | **2** ✓ |
| 触发器函数 `set_updated_at()` | 1 | **1** ✓ |
| UNIQUE + CHECK 约束 | ≥30 | **33** ✓(含 NULLS NOT DISTINCT) |
| 索引 | ≥35 | **38** ✓(含 partial idx_users_state / idx_device_sessions_revoked_at) |
| 外键 | ≥20 | **23** ✓(全 ON DELETE CASCADE / RESTRICT / SET NULL 正确) |
| `uniq_messages_idem` 启用 NULLS NOT DISTINCT | yes (PG 15+ 特性,18.6 必支持) | **yes** ✓(per 0005 注释 2026-08-23 自审保留) |
| pgcrypto 扩展 | 启用 | **1.4** ✓ |

## 4. Insert 烟测

```sql
BEGIN;
INSERT INTO tenants (id, name) VALUES (gen_random_uuid(), 'B1-Verify-Studio') RETURNING id;
INSERT INTO games   (id, tenant_id, name) VALUES (gen_random_uuid(), <tenant_id>, 'B1-Verify-Game') RETURNING id;
INSERT INTO environments (id, game_id, name) VALUES (gen_random_uuid(), <game_id>, 'test') RETURNING id;
INSERT INTO users (id, environment_id, kind) VALUES (gen_random_uuid(), <env_id>, 'guest') RETURNING id;
COMMIT;
```

→ 全部 `INSERT 0 1`,事务 `COMMIT` 成功。FKey 链无中断。

## 5. 已知缺口(per 缺标比错标原则)

| # | 缺口 | 影响 | 建议 |
|---|---|---|---|
| 1 | 没用 `sqlx migrate run` 走迁移器,只用了 `psql -f` 单文件执行 | E-2 集成测时需 `sqlx-cli` 路径再验一次 | C-1 实装完后,补充 `sqlx migrate run --source migrations/` 端到端 |
| 2 | 本地 init 集群用 `trust` auth + port 5544,**不是 K3s 上的 PG 18.6** | F-1 修好后,需在 K3s 集群内复跑一次同 6 份 SQL | F-1 解锁后,由 F-2 任务覆盖 |
| 3 | `uniq_users_env_extid` 未实测 NULL 行为(只验了定义) | P1-2 修复(2026-08-23 自审)语义需 insert 2 Guest 验 | C-1 集成测补 |
| 4 | 触发器 `set_updated_at()` 未实测自维护 | 需 UPDATE 一次验 `updated_at = now()` | C-1 集成测补 |
| 5 | 没验 `messages` 的 `reply_to` 自引用 FK ON DELETE SET NULL | 需 INSERT 两条,删 first,看 second.reply_to=NULL | C-1 集成测补 |
| 6 | `audit_logs.tenant_id` 实际没有 FK → tenants.id(per 0006 注释"系统日志可跨租户") | 可能需 V1 评估加 FK | 暂保留设计,V1 评估 |

## 6. 交付物清单

| 路径 | 说明 |
|---|---|
| `scripts/init-pg18-b1.sh` | WSL Ubuntu 本地 PG 18.6 init 启动脚本(pg_ctl + port 5544) |
| `scripts/verify-pg18-b1.sh` | 11 项 schema 核验脚本(tables/triggers/constraints/indexes/FKs/insert/idempotency/version) |
| `tests/pg18-b1-verify-output.log` | 11 项核验完整输出(commit 进 git) |
| `tests/pg18-b1-migration-report.md` | 本文件 |

## 7. 关联文档

- 上游:`docs/132-wbs.md` §5.2 B-1
- 平行:`docs/deployment-bridge-known-issue.md`(F-1 Blocker)
- 下游:C-1(`im-core` 6 个 PgRepository 实装,直接用这 14 张表)
- 字段依据:`docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md` §F.1-F.14

## 8. 修订记录

| 版本 | 日期 | 修订人 | 内容 |
|---|---|---|---|
| 1.0.0 | 2026-09-01 | 架构师 (Mavis 接手 agent per DEC-008) | 初版:6 份 SQL 在真 PG 18.6 上 6/6 通过,14 表 / 33 约束 / 38 索引 / 23 FK 全部落地,insert 烟测通过 |
