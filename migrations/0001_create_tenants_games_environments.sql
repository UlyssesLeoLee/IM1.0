-- +migrate Up
-- ============================================================================
-- 0001: tenants / games / environments + updated_at 触发器函数
-- 依据: docs/DetailedDesign.md §2 / docs/BasicDesign.md §4
-- 字段字典: docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md §F.1-F.3
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;  -- gen_random_uuid() (PG 13+ 可选内置,16+ 内置)

-- tenants: 平台客户(发行商/工作室)
CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL CHECK (length(name) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- games: 租户下的具体游戏
CREATE TABLE IF NOT EXISTS games (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (length(name) <= 128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, name)
);
CREATE INDEX IF NOT EXISTS idx_games_tenant_id ON games(tenant_id);

-- environments: 游戏下的环境(production/staging/test)
-- settings JSONB 字段: 9 项见 BasicDesign.md §14.3
-- (friend_system_enabled / rate_limit.* / message.recall_window_seconds /
--  message.retention_days.* / voice.enabled / audit.detailed)
CREATE TABLE IF NOT EXISTS environments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE RESTRICT,
    name TEXT NOT NULL CHECK (name IN ('production', 'staging', 'test')),
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (game_id, name),
    CONSTRAINT chk_environments_settings_is_object CHECK (jsonb_typeof(settings) = 'object')
);
CREATE INDEX IF NOT EXISTS idx_environments_game_id ON environments(game_id);

-- shared trigger function: 自动维护 updated_at
CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_environments_before_update
BEFORE UPDATE ON environments
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
