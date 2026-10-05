-- +migrate Up
-- ============================================================================
-- 0008: dlq_records —— 领域事件 DLQ 的 **长留存** 层
-- 依据: aux-08 §D.3 存储策略(第 2 行) + §K GAP-2
--       + DetailedDesign §9.1「失败不阻塞 ack 但写入 DLQ」
--
-- ## 为什么是**专表**而不是 audit_logs
--
-- aux-08 §D.3 的 MVP 原文是「NATS DLQ subject + `audit_logs(action=dlq_record,
-- detail=JSONB)`」, 而同一节的表格把 `dlq_records` 标为「V1+ 加, V1 预留」。
-- GAP-2 进一步写明「`dlq_records` 专表 V1+ 才建, MVP 阶段用
-- `audit_logs.detail` JSONB 替代 —— DLQ 检索不友好」。
--
-- 2026-10-06 架构拍板: 走**专表**。理由不是「专表更好」, 而是 MVP 的那条路
-- **在 schema 上走不通**:
--
--   - `audit_logs.tenant_id UUID NOT NULL`(migrations/0006:15)
--   - `audit_logs.target_type TEXT NOT NULL CHECK (target_type IN
--     ('user','conversation','environment','extension','secret','system'))`
--     (migrations/0006:18) —— 6 值枚举, **不含**「事件」
--
-- 而事件发布路径**拿不到租户**(`publish(topic, payload: &[u8])` 只有裸字节,
-- 在 `tx.commit()` 之后调用)。要凑出 tenant_id 只有两条路: 改
-- `EventPublisher::publish` 签名把上下文带进去(触及全部调用点), 或在**失败
-- 路径**上从 payload 解析 conversation_id 再查 environment→game→tenant
-- (多一次 DB 往返, 且让 publisher 耦合 DB 仓储)。两者都比重开一张表更贵。
--
-- ## 分类 (per 用户的 DB 三分类横展原则)
--
-- **Transaction(事件流水) + Work(待重放队列)**, 不是 Master, 更不是 audit。
-- 理由: 主键 `dlq_id` 由写入方生成, 表按**追加**为主; 但它同时带
-- `replayed_at` / `discarded_at` / `replay_attempts` 三个列 —— 这组列让一行
-- 记录能从「待重放」走到「已重放 / 已丢弃」, 生命周期与普通事件流水不同。
-- 完成后应清理(见 aux-08 §D.4 的人工处置流程)。
--
-- `audit_logs` 是「谁做了什么」的**审计**轨迹, 与「这条事件待重放」是两种
-- 生命周期、两种保留期、两种读者。把工作队列塞进审计表会让两边都变难查。
--
-- ## 索引按 aux-01 §D 逐字拼出列名
--
-- §1.26 记录了 12 个既有索引名缩写了列名。本表刻意拼全:
-- `idx_dlq_records_original_task_failed_at` / `idx_dlq_records_failed_at` /
-- `idx_dlq_records_pending_replay_failed_at`。**既有索引的重命名触及迁移历史
-- (aux-01 §H 规定迁移「永远追加, 不改历史」), 不在本迁移范围内** —— 新表
-- 至少不该再欠一笔。
--
-- ## 三个恒为 NULL 的列
--
-- `error_stack` / `context_trace_id` / `context_user_id` / `context_env_id`
-- 对齐 `aux-08 §D.2` 的 `DlqRecord`。它们**恒为 NULL** 不是漏填: 规范写
-- 「stack trace, 脱敏后」而仓内**没有**脱敏, 把未脱敏的栈写进一张运维要读的
-- 长期表是净风险; trace/user/env 则在 `tx.commit()` 之后已无请求上下文,
-- 编造一个值会让排障被错误信息带偏。列留着, 宁可空着。
-- ============================================================================

CREATE TABLE IF NOT EXISTS dlq_records (
    dlq_id                 UUID PRIMARY KEY,
    -- aux-08 §D.2 `original_task`: 事件语境下即 subject
    original_task          TEXT NOT NULL,
    -- 原始载荷。解析失败时写入 JSON **字符串**(见 DlqRecord 的说明:
    -- 形状不统一好过丢数据)
    original_payload       JSONB NOT NULL,
    -- aux-08 §D.2 `error` 子结构, 摊平成列
    error_code             TEXT NOT NULL,
    error_message          TEXT NOT NULL,
    error_stack            TEXT,
    error_http_status      SMALLINT NOT NULL,
    -- aux-08 §D.2 `context` 子结构
    context_trace_id       TEXT,
    context_user_id        TEXT,
    context_env_id         TEXT,
    context_attempt_count  INTEGER NOT NULL,
    context_first_attempt_at TIMESTAMPTZ NOT NULL,
    context_last_attempt_at  TIMESTAMPTZ NOT NULL,
    failed_at              TIMESTAMPTZ NOT NULL,
    -- aux-08 §D.2 `dlq_destination`, 形如 `dlq.event.im.message.created`
    dlq_destination        TEXT NOT NULL,
    -- ---- 重放簿记(见上方「分类」段) ----
    replay_attempts        INTEGER NOT NULL DEFAULT 0,
    replayed_at            TIMESTAMPTZ,
    discarded_at           TIMESTAMPTZ,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- HTTP 状态码取值域: DB 层兜住, 应用层漏掉时不要静默写进 0 或 700
    CONSTRAINT dlq_records_error_http_status_check
        CHECK (error_http_status BETWEEN 100 AND 599),
    -- 重放簿记的最小一致性: 重放次数不能是负数
    CONSTRAINT dlq_records_replay_attempts_check
        CHECK (replay_attempts >= 0)
);

-- 排障主查询: 「某类事件最近死信了多少」 —— 与 aux-08 §D.4 的 `dlq list` 对应
CREATE INDEX IF NOT EXISTS idx_dlq_records_original_task_failed_at
    ON dlq_records (original_task, failed_at DESC);

-- 全局时间线(按时间扫全表)
CREATE INDEX IF NOT EXISTS idx_dlq_records_failed_at
    ON dlq_records (failed_at DESC);

-- 待重放队列: 只索引既未重放也未丢弃的行(partial, 与 0002 idx_users_state /
-- 0007 idx_users_username 风格一致)
CREATE INDEX IF NOT EXISTS idx_dlq_records_pending_replay_failed_at
    ON dlq_records (failed_at)
    WHERE replayed_at IS NULL AND discarded_at IS NULL;

-- +migrate Down
-- DROP INDEX IF EXISTS idx_dlq_records_pending_replay_failed_at;
-- DROP INDEX IF EXISTS idx_dlq_records_failed_at;
-- DROP INDEX IF EXISTS idx_dlq_records_original_task_failed_at;
-- DROP TABLE IF EXISTS dlq_records;
