-- +migrate Up
-- ============================================================================
-- 0007: users 增加 username + password_hash (用于密码登录认证)
-- 依据: SRS §11 IM-ID-001~005 + C-4 WBS(ULYS-145)需求
--
-- 设计要点:
-- 1. username 与 password_hash 都设为可空(NOT NULL 不强制,保留原有 Guest/external-identity 注册路径)
-- 2. username 在 (environment_id, username) 上 UNIQUE —— 同 env 下用户名唯一
-- 3. PG 默认 NULL distinct 行为:允许多个 NULL username 共存(不影响原有 Guest + 部分 external user)
-- 4. password_hash 用 argon2id 字符串(标准 PHC 格式 $argon2id$v=19$m=...,t=...,p=1$...)
-- 5. 用户名规则由应用层 enforce(MVP: 3-32 字符, [a-zA-Z0-9_-]);不在 DB 强约束以免阻塞其他接入
-- 6. 不删原字段;不改原 UNIQUE(environment_id, external_identity)约束
--
-- 风险:加列不带 DEFAULT(非 NOT NULL 不会锁表);索引创建用 CONCURRENTLY(INDEX 后必须 VACUUM)
-- ============================================================================

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS username TEXT,
    ADD COLUMN IF NOT EXISTS password_hash TEXT;

-- 长度约束(防止异常长字符串污染存储)
ALTER TABLE users
    DROP CONSTRAINT IF EXISTS chk_users_username_len;
ALTER TABLE users
    ADD CONSTRAINT chk_users_username_len
    CHECK (username IS NULL OR (length(username) BETWEEN 3 AND 64));

ALTER TABLE users
    DROP CONSTRAINT IF EXISTS chk_users_password_hash_len;
ALTER TABLE users
    ADD CONSTRAINT chk_users_password_hash_len
    CHECK (password_hash IS NULL OR length(password_hash) <= 256);

-- (env, username) 联合唯一;NULL 视为 distinct(PG 默认),允许多 Guest 共享 NULL username
-- 用普通索引创建(非 CONCURRENTLY,因为迁移脚本假定在低负载维护窗口运行;V1+ 上线可改 CONCURRENTLY)
CREATE UNIQUE INDEX IF NOT EXISTS uniq_users_env_username
    ON users (environment_id, username);

-- 加快 username 查询的覆盖索引
CREATE INDEX IF NOT EXISTS idx_users_username
    ON users (environment_id, username)
    WHERE username IS NOT NULL;

-- +migrate Down
-- ============================================================================
-- 回滚(谨慎:删列会丢数据,仅本地开发 + CI 集成测试用)
-- ============================================================================

-- DROP INDEX IF EXISTS idx_users_username;
-- DROP INDEX IF EXISTS uniq_users_env_username;
-- ALTER TABLE users
--     DROP CONSTRAINT IF EXISTS chk_users_password_hash_len,
--     DROP CONSTRAINT IF EXISTS chk_users_username_len,
--     DROP COLUMN IF EXISTS password_hash,
--     DROP COLUMN IF EXISTS username;