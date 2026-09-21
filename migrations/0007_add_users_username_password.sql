-- +migrate Up
-- ============================================================================
-- 0007: users 表新增 username / password_hash 列(C-3 + C-4 整合, 方案 C)
-- 依据: SRS §11 IM-ID-001~005 + DetailedDesign §9.2 + ImplementationSpec §7.4.1
--       docs/132-wbs.md §5.3 C-3 + C-4 WBS
--
-- 设计决策 (per 架构拍板方案 C, 2026-09-21 JST):
--   - username 长度 3-64 (兼容企业 SSO UPN/DN, 64 是 V1+ 扩展空间)
--   - 字符集 [a-zA-Z0-9_-] **DB 层 regex 强制** (DB 是真理之源, 防止应用层漏掉)
--   - 唯一索引 partial `WHERE username IS NOT NULL`
--     (与 0002 uniq_users_env_extid 风格一致: NULL 视为 distinct, 允许多 Guest/extid user 共存)
--   - password_hash 长度 ≤256 (argon2 PHC-format 上限保护)
--
-- 与 C-3 (6e3b0f1) 的差异: 长度 3-32 → 3-64
-- 与 C-4 (b47d2ca) 的差异: 加 DB regex 强制 + partial unique (替代 full unique)
--
-- 兼容性:
--   - 应用层 (C-3 validate_username): 字符集 regex 仍为 [a-zA-Z0-9_-], 长度上限改 64
--   - 应用层 (C-4 register/authenticate): SQL 不依赖 CHECK, 自动兼容
--   - INSERT 路径 (pg.rs create_with_password / register_with_password): SQL RETURNING 列不变
-- ============================================================================

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS username TEXT
        CHECK (username IS NULL OR (length(username) BETWEEN 3 AND 64
              AND username ~ '^[a-zA-Z0-9_-]+$')),
    ADD COLUMN IF NOT EXISTS password_hash TEXT
        CHECK (password_hash IS NULL OR length(password_hash) <= 256);

-- 同一 environment 下 username 唯一(NULL 视为 distinct,允许多 Guest + extid user 共存)
-- partial unique 索引与 0002 uniq_users_env_extid 风格一致
CREATE UNIQUE INDEX IF NOT EXISTS uniq_users_env_username
    ON users (environment_id, username)
    WHERE username IS NOT NULL;

-- username 查找覆盖索引(仅索引非 NULL 行,与 partial unique 风格一致)
CREATE INDEX IF NOT EXISTS idx_users_username
    ON users (username)
    WHERE username IS NOT NULL;

-- +migrate Down
-- ALTER TABLE users DROP COLUMN IF EXISTS password_hash, DROP COLUMN IF EXISTS username;
-- DROP INDEX IF EXISTS idx_users_username;
-- DROP INDEX IF EXISTS uniq_users_env_username;
