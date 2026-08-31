-- ============================================================================
-- tenant_fixture.sql
-- Maps to: migrations/0001_create_tenants_games_environments.sql
-- Tables: tenants, games, environments
-- Idempotent: ON CONFLICT DO NOTHING for all inserts.
-- UUIDs: deterministic for reproducible test runs.
-- ============================================================================

INSERT INTO tenants (id, name, created_at) VALUES
    ('11111111-1111-4111-8111-111111111111', 'acme-games',       '2026-08-23T00:00:00Z'),
    ('22222222-2222-4222-8222-222222222222', 'beta-studios',     '2026-08-23T00:00:00Z')
ON CONFLICT (id) DO NOTHING;

INSERT INTO games (id, tenant_id, name, created_at) VALUES
    ('33333333-3333-4333-8333-333333333333', '11111111-1111-4111-8111-111111111111', 'im-test-game', '2026-08-23T00:00:00Z')
ON CONFLICT (id) DO NOTHING;

INSERT INTO environments (id, game_id, name, settings, created_at, updated_at) VALUES
    (
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '33333333-3333-4333-8333-333333333333',
        'test',
        '{
            "friend_system_enabled": true,
            "rate_limit.send_message.per_user_per_minute": 60,
            "rate_limit.token_exchange.per_server_per_minute": 600,
            "message.recall_window_seconds": 120,
            "message.retention_days.dm": 30,
            "message.retention_days.group": 90,
            "message.retention_days.channel": -1,
            "voice.enabled": false,
            "audit.detailed": true
        }'::jsonb,
        '2026-08-23T00:00:00Z',
        '2026-08-23T00:00:00Z'
    )
ON CONFLICT (id) DO NOTHING;
