-- +migrate Up
-- ============================================================================
-- 0007: users 表新增 username / password_hash 列(C-3 WBS)
-- 依据: SRS §11 IM-ID-001~004 (Identity 三类主体: User 含 password 路径)
--       DetailedDesign §9.2 Identity 模块 + ImplementationSpec §7.4.1
--       docs/132-wbs.md §5.3 C-3 IdentityService::register 实装
--
-- 设计要点:
--   - username NULLABLE: 兼容现有 User (extid-based) + Guest (无 username)
--     实装 register 时强制要求 NOT NULL(username-based user)
--   - 同一 environment 下 username 唯一(NULL 视为 distinct,与 extid 约束对齐)
--   - password_hash 存储 argon2id 字符串 (PHC-format, 由 argon2 crate 输出)
--     NULL 表示该 user 走 extid/OAuth 路径(无密码登录)
--   - username 长度 3-32,只允许 [a-zA-Z0-9_-]
-- ============================================================================

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS username TEXT
        CHECK (username IS NULL OR (length(username) BETWEEN 3 AND 32
              AND username ~ '^[a-zA-Z0-9_-]+$')),
    ADD COLUMN IF NOT EXISTS password_hash TEXT
        CHECK (password_hash IS NULL OR length(password_hash) <= 256);

-- 同一 environment 下 username 唯一(NULL 视为 distinct,允许多 Guest + extid user 共存)
CREATE UNIQUE INDEX IF NOT EXISTS uniq_users_env_username
    ON users (environment_id, username)
    WHERE username IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_users_username
    ON users (username)
    WHERE username IS NOT NULL;

-- +migrate Down
-- ALTER TABLE users DROP COLUMN IF EXISTS password_hash, DROP COLUMN IF EXISTS username;
-- DROP INDEX IF EXISTS idx_users_username;
-- DROP INDEX IF EXISTS uniq_users_env_username;
