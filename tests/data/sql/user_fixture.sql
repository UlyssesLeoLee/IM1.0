-- ============================================================================
-- user_fixture.sql
-- Maps to: migrations/0002_create_users_device_sessions.sql
-- Tables: users, device_sessions
-- Idempotent: ON CONFLICT DO NOTHING.
-- NOTE: device_sessions.refresh_token_hash is the argon2id hash; the
-- fixture uses a literal marker so tests can detect mismatch attempts.
-- ============================================================================

INSERT INTO users (id, environment_id, kind, external_identity, state, display_name, created_at) VALUES
    -- Regular user (steam-extid)
    (
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        'user',
        '{"provider": "steam", "uid": "76561198000000000"}'::jsonb,
        'active',
        'Player1',
        '2026-08-23T00:00:00Z'
    ),
    -- Second regular user
    (
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        'user',
        '{"provider": "steam", "uid": "76561198000000001"}'::jsonb,
        'active',
        'Player2',
        '2026-08-23T00:00:00Z'
    ),
    -- Guest (external_identity = NULL per migration 0002 self-audit fix)
    (
        '3c4e6679-7425-40de-944b-e07fc1f90ae7',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        'guest',
        NULL,
        'active',
        'Guest1',
        '2026-08-23T00:00:00Z'
    )
ON CONFLICT (id) DO NOTHING;

INSERT INTO device_sessions (id, user_id, device_fingerprint, refresh_token_hash, created_at, revoked_at) VALUES
    (
        '9b8e6679-7425-40de-944b-e07fc1f90ae7',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        'sha256:fixture-fingerprint-player1',
        'argon2id$FIXTURE-PLAYER1-DO-NOT-USE-IN-PROD',
        '2026-08-23T00:00:00Z',
        NULL
    )
ON CONFLICT (id) DO NOTHING;
