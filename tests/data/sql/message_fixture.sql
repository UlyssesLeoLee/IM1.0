-- ============================================================================
-- message_fixture.sql
-- Maps to: migrations/0005_create_messages_reactions.sql
-- Tables: messages, message_reactions
-- Idempotent: ON CONFLICT DO NOTHING.
-- sequence is hard-monotonic per conversation_id (UNIQUE).
-- ============================================================================

-- 3 messages in the group conversation.
INSERT INTO messages (id, conversation_id, sequence, sender_id, kind, content, reply_to, idempotency_key, state, created_at, edited_at) VALUES
    (
        '8a7e6679-7425-40de-944b-e07fc1f90ae7',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        1,
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        'text',
        '{"text": "你好, group!"}'::jsonb,
        NULL,
        'idem-msg-001',
        'sent',
        '2026-08-23T00:00:00Z',
        NULL
    ),
    (
        '8a7e6679-7425-40de-944b-e07fc1f90ae8',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        2,
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        'text',
        '{"text": "Hello to you too"}'::jsonb,
        '8a7e6679-7425-40de-944b-e07fc1f90ae7',
        'idem-msg-002',
        'read',
        '2026-08-23T00:00:01Z',
        NULL
    ),
    (
        '8a7e6679-7425-40de-944b-e07fc1f90ae9',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        3,
        NULL,                                       -- system message (sender=NULL)
        'system',
        '{"event": "join", "actor_user_id": "3c4e6679-7425-40de-944b-e07fc1f90ae7"}'::jsonb,
        NULL,
        'idem-msg-003',
        'sent',
        '2026-08-23T00:00:02Z',
        NULL
    )
ON CONFLICT (id) DO NOTHING;

-- Bump sequence counters (idempotent: UPDATE only if not yet advanced).
UPDATE conversation_sequences
SET next_sequence = 4
WHERE conversation_id = '7c9e6679-7425-40de-944b-e07fc1f90ae7'
  AND next_sequence < 4;

-- One reaction on message 001.
INSERT INTO message_reactions (message_id, user_id, emoji, created_at) VALUES
    (
        '8a7e6679-7425-40de-944b-e07fc1f90ae7',
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        '👍',
        '2026-08-23T00:00:30Z'
    )
ON CONFLICT (message_id, user_id, emoji) DO NOTHING;
