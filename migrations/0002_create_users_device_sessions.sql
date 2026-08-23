-- +migrate Up
-- ============================================================================
-- 0002: users / device_sessions
-- 依据: docs/DetailedDesign.md §2 / docs/BasicDesign.md §4
-- 字段字典: docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md §F.4-F.5
--
-- ⚠️ 2026-08-23 自审修复:
-- users.external_identity UNIQUE 不得用 NULLS NOT DISTINCT
-- (Guest extid=NULL, PG 15+ NULLS NOT DISTINCT 会阻断多 Guest 共存)
-- 采用 PG 默认语义(NULL 视为 distinct),允许多 Guest / 1 user-per-(env,extid)
-- ============================================================================

CREATE TABLE IF NOT EXISTS users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('user', 'guest')),
    external_identity JSONB,           -- Guest 必为 NULL;User 必填
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'banned', 'suspended', 'deleted')),
    display_name TEXT CHECK (length(display_name) <= 64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- (env, external_identity) 联合唯一;NULL 视为 distinct(PG 默认行为)
    -- 同 env 下只允许 1 个 user 拥有特定 extid;但允许多个 Guest (extid=NULL)
    CONSTRAINT uniq_users_env_extid UNIQUE (environment_id, external_identity)
);
CREATE INDEX IF NOT EXISTS idx_users_environment_id ON users(environment_id);
CREATE INDEX IF NOT EXISTS idx_users_state ON users(state) WHERE state != 'active';

CREATE TABLE IF NOT EXISTS device_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_fingerprint TEXT CHECK (length(device_fingerprint) <= 256),
    refresh_token_hash TEXT NOT NULL,    -- argon2 hash + pepper
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    -- 同一 user 下 active refresh_token_hash 不重复(revoke 即可重发)
    CONSTRAINT uniq_device_sessions_active_refresh UNIQUE (user_id, refresh_token_hash)
);
CREATE INDEX IF NOT EXISTS idx_device_sessions_user_id ON device_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_device_sessions_revoked_at ON device_sessions(revoked_at) WHERE revoked_at IS NULL;
