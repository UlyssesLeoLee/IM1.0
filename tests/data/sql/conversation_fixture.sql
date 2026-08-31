-- ============================================================================
-- conversation_fixture.sql
-- Maps to: migrations/0004_create_conversations_sequences_members_dm_pairs.sql
-- Tables: conversations, conversation_sequences, conversation_members, dm_pairs
-- Idempotent: ON CONFLICT DO NOTHING.
-- ============================================================================

-- One group conversation (3 members) + one DM (player1 <-> player2).
INSERT INTO conversations (id, environment_id, kind, metadata, created_at) VALUES
    (
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        'group',
        '{"game.type": "guild", "game.game_id": "5d5e6679-7425-40de-944b-e07fc1f90ae7"}'::jsonb,
        '2026-08-23T00:00:00Z'
    ),
    (
        '6b7e6679-7425-40de-944b-e07fc1f90ae7',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        'dm',
        '{}'::jsonb,
        '2026-08-23T00:00:00Z'
    )
ON CONFLICT (id) DO NOTHING;

-- Strong-monotonic sequence allocator rows (one per conversation).
INSERT INTO conversation_sequences (conversation_id, next_sequence) VALUES
    ('7c9e6679-7425-40de-944b-e07fc1f90ae7', 1),
    ('6b7e6679-7425-40de-944b-e07fc1f90ae7', 1)
ON CONFLICT (conversation_id) DO NOTHING;

-- Group: 3 members; DM: 2 members.
INSERT INTO conversation_members (conversation_id, user_id, role, joined_at, last_read_sequence) VALUES
    ('7c9e6679-7425-40de-944b-e07fc1f90ae7', '1a2e6679-7425-40de-944b-e07fc1f90ae7', 'owner',  '2026-08-23T00:00:00Z', 0),
    ('7c9e6679-7425-40de-944b-e07fc1f90ae7', '2b3e6679-7425-40de-944b-e07fc1f90ae7', 'member', '2026-08-23T00:00:00Z', 0),
    ('7c9e6679-7425-40de-944b-e07fc1f90ae7', '3c4e6679-7425-40de-944b-e07fc1f90ae7', 'member', '2026-08-23T00:00:00Z', 0),
    ('6b7e6679-7425-40de-944b-e07fc1f90ae7', '1a2e6679-7425-40de-944b-e07fc1f90ae7', 'owner',  '2026-08-23T00:00:00Z', 0),
    ('6b7e6679-7425-40de-944b-e07fc1f90ae7', '2b3e6679-7425-40de-944b-e07fc1f90ae7', 'member', '2026-08-23T00:00:00Z', 0)
ON CONFLICT (conversation_id, user_id) DO NOTHING;

-- DM: dm_pairs normalized (user_a < user_b). Player1 < Player2 in UUID lexicographic order.
INSERT INTO dm_pairs (environment_id, user_a, user_b, conversation_id) VALUES
    (
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        '6b7e6679-7425-40de-944b-e07fc1f90ae7'
    )
ON CONFLICT (environment_id, user_a, user_b) DO NOTHING;
