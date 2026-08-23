-- +migrate Up
-- ============================================================================
-- 0005: messages / message_reactions
-- 依据: docs/DetailedDesign.md §2 / docs/BasicDesign.md §4
-- 字段字典: docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md §F.12-F.13
--
-- 关键设计:
-- - messages.sequence 强单调(行锁 + UNIQUE 联合)
-- - messages.idempotency_key UNIQUE NULLS NOT DISTINCT (允许系统消息 sender=NULL 也要去重)
-- - reply_to 自引用 FK,ON DELETE SET NULL(消息被删,引用不悬空)
-- ============================================================================

CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL CHECK (sequence >= 1),
    sender_id UUID REFERENCES users(id) ON DELETE SET NULL,  -- NULL = 系统消息
    kind TEXT NOT NULL CHECK (kind IN ('text', 'image', 'file', 'sticker', 'system', 'custom')),
    content JSONB NOT NULL,                 -- 按 kind 校验,见 aux-13 §4.1
    reply_to UUID REFERENCES messages(id) ON DELETE SET NULL,
    idempotency_key TEXT NOT NULL CHECK (length(idempotency_key) <= 128),
    state TEXT NOT NULL DEFAULT 'sent' CHECK (state IN ('sent', 'delivered', 'read', 'recalled', 'deleted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at TIMESTAMPTZ,
    UNIQUE (conversation_id, sequence),
    -- 幂等键唯一;系统消息 sender=NULL 也要去重(2026-08-23 自审保留 NULLS NOT DISTINCT)
    CONSTRAINT uniq_messages_idem UNIQUE NULLS NOT DISTINCT (conversation_id, sender_id, idempotency_key),
    CONSTRAINT chk_messages_content_is_object CHECK (jsonb_typeof(content) = 'object')
);

-- 核心查询路径:增量拉取 (conversation_id, after_sequence)
CREATE INDEX IF NOT EXISTS idx_messages_conversation_seq ON messages(conversation_id, sequence);

-- 全文搜索预留(V1+ IM-SEARCH-001,MVP 暂不接 GIN)
-- CREATE INDEX IF NOT EXISTS idx_messages_content_fts ON messages USING GIN (to_tsvector('simple', content::text));

CREATE TABLE IF NOT EXISTS message_reactions (
    message_id UUID NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    emoji TEXT NOT NULL CHECK (length(emoji) <= 32),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (message_id, user_id, emoji)
);
CREATE INDEX IF NOT EXISTS idx_message_reactions_message ON message_reactions(message_id);
