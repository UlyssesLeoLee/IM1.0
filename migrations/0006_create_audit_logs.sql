-- +migrate Up
-- ============================================================================
-- 0006: audit_logs
-- 依据: docs/DetailedDesign.md §2 / docs/BasicDesign.md §4
-- 字段字典: docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md §F.14
--
-- 关键设计:
-- - actor_id 可为 NULL(系统/匿名操作)
-- - detail JSONB 写入前应用层过滤 password/token/secret
-- - V1+ 按 created_at RANGE 分区(预留,MVP 单表)
-- ============================================================================

CREATE TABLE IF NOT EXISTS audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    actor_id UUID,                        -- 用户注销后由应用层置 NULL
    action TEXT NOT NULL,                 -- 见 aux-02 §D audit_logs.action
    target_type TEXT NOT NULL CHECK (target_type IN ('user', 'conversation', 'environment', 'extension', 'secret', 'system')),
    target_id UUID,
    detail JSONB,                         -- 应用层脱敏后写入
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_audit_logs_tenant_created ON audit_logs(tenant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_logs_action ON audit_logs(action);
-- audit 专用分区(V1+ 启用,MVP 不分区):
-- ALTER TABLE audit_logs PARTITION BY RANGE (created_at);
