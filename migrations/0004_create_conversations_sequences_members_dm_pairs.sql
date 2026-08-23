-- +migrate Up
-- ============================================================================
-- 0004: conversations / conversation_sequences / dm_pairs / conversation_members
-- 依据: docs/DetailedDesign.md §2 / docs/BasicDesign.md §4
-- 字段字典: docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md §F.8-F.11
--
-- 关键设计:
-- - conversations.metadata 严禁游戏专有字段(只有 game.*/ai.*/work.* 命名空间)
-- - conversation_sequences 强单调行锁(SELECT ... FOR UPDATE)
-- - dm_pairs 用 CHECK(user_a < user_b) 规范化双人私聊
-- ============================================================================

CREATE TABLE IF NOT EXISTS conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    kind TEXT NOT NULL CHECK (kind IN ('dm', 'group', 'channel', 'system', 'broadcast')),
    metadata JSONB NOT NULL DEFAULT '{}',     -- 命名空间约定: game.* / ai.* / work.*
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT chk_conversations_metadata_is_object CHECK (jsonb_typeof(metadata) = 'object')
);
CREATE INDEX IF NOT EXISTS idx_conversations_environment_id ON conversations(environment_id, created_at DESC);

-- 强单调 sequence 分配器(行锁,详见 ImplementationSpec §4.5)
CREATE TABLE IF NOT EXISTS conversation_sequences (
    conversation_id UUID PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    next_sequence BIGINT NOT NULL DEFAULT 1 CHECK (next_sequence >= 1)
);

-- DM 唯一对(user_a < user_b)
CREATE TABLE IF NOT EXISTS dm_pairs (
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    user_a UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    user_b UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    PRIMARY KEY (environment_id, user_a, user_b),
    UNIQUE (conversation_id),
    CHECK (user_a < user_b)
);
CREATE INDEX IF NOT EXISTS idx_dm_pairs_conversation ON dm_pairs(conversation_id);

-- 会话成员
CREATE TABLE IF NOT EXISTS conversation_members (
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'admin', 'member')),
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_read_sequence BIGINT NOT NULL DEFAULT 0 CHECK (last_read_sequence >= 0),
    PRIMARY KEY (conversation_id, user_id)
);
-- "我的会话列表"查询专用索引
CREATE INDEX IF NOT EXISTS idx_conversation_members_user_id ON conversation_members(user_id);
