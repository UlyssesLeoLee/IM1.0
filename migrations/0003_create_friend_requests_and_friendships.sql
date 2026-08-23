-- +migrate Up
-- ============================================================================
-- 0003: friend_requests / friendships
-- 依据: docs/DetailedDesign.md §2 / docs/BasicDesign.md §4
-- 字段字典: docs/templates/04-detailed-design/aux/aux-02-data-dictionary.md §F.6-F.7
--
-- ⚠️ 2026-08-23 自审 P2-1 已知局限:
-- friend_requests UNIQUE(env, sender, recipient) 限制"同对用户只能发 1 次申请(跨所有 state)"
-- 若产品后续要求"被拒后可重发",改为 partial UNIQUE WHERE state='pending'
-- 编码前向 PM 确认
-- ============================================================================

CREATE TABLE IF NOT EXISTS friend_requests (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    sender_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'accepted', 'rejected', 'expired')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (environment_id, sender_id, recipient_id),
    CHECK (sender_id <> recipient_id)
);
CREATE INDEX IF NOT EXISTS idx_friend_requests_recipient ON friend_requests(recipient_id, state) WHERE state = 'pending';
CREATE INDEX IF NOT EXISTS idx_friend_requests_sender ON friend_requests(sender_id, state);

CREATE TRIGGER trg_friend_requests_before_update
BEFORE UPDATE ON friend_requests
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TABLE IF NOT EXISTS friendships (
    environment_id UUID NOT NULL REFERENCES environments(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    friend_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    state TEXT NOT NULL CHECK (state IN ('accepted', 'blocked')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (environment_id, user_id, friend_id),
    CHECK (user_id <> friend_id)
);
CREATE INDEX IF NOT EXISTS idx_friendships_user ON friendships(user_id, state);
