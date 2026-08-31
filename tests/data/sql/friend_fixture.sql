-- ============================================================================
-- friend_fixture.sql
-- Maps to: migrations/0003_create_friend_requests_and_friendships.sql
-- Tables: friend_requests, friendships
-- Idempotent: ON CONFLICT DO NOTHING.
-- Known limitation in 0003: UNIQUE(env, sender, recipient) covers all states.
-- ============================================================================

INSERT INTO friend_requests (id, environment_id, sender_id, recipient_id, state, created_at, updated_at) VALUES
    (
        '44444444-4444-4444-8444-444444444444',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        'accepted',
        '2026-08-23T00:00:00Z',
        '2026-08-23T00:00:01Z'
    ),
    (
        '55555555-5555-4555-8555-555555555555',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '3c4e6679-7425-40de-944b-e07fc1f90ae7',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        'pending',
        '2026-08-23T00:00:00Z',
        '2026-08-23T00:00:00Z'
    )
ON CONFLICT (id) DO NOTHING;

-- friendships PK = (environment_id, user_id, friend_id); one row per direction.
-- state IN {accepted, blocked}.
INSERT INTO friendships (environment_id, user_id, friend_id, state, created_at) VALUES
    (
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        'accepted',
        '2026-08-23T00:00:01Z'
    ),
    (
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        'accepted',
        '2026-08-23T00:00:01Z'
    )
ON CONFLICT (environment_id, user_id, friend_id) DO NOTHING;
