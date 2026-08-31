-- ============================================================================
-- audit_log_fixture.sql
-- Maps to: migrations/0006_create_audit_logs.sql
-- Table: audit_logs
-- Idempotent: ON CONFLICT DO NOTHING.
-- detail JSONB is application-layer redacted (no password/token/secret).
-- ============================================================================

INSERT INTO audit_logs (id, tenant_id, actor_id, action, target_type, target_id, detail, created_at) VALUES
    (
        'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa',
        '11111111-1111-4111-8111-111111111111',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        'auth.token.exchange',
        'user',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        '{"external_provider": "steam", "external_uid": "76561198000000000"}'::jsonb,
        '2026-08-23T00:00:00Z'
    ),
    (
        'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
        '11111111-1111-4111-8111-111111111111',
        '1a2e6679-7425-40de-944b-e07fc1f90ae7',
        'conversation.create',
        'conversation',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '{"kind": "group", "member_count": 3}'::jsonb,
        '2026-08-23T00:00:00Z'
    ),
    (
        'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
        '11111111-1111-4111-8111-111111111111',
        '2b3e6679-7425-40de-944b-e07fc1f90ae7',
        'message.send',
        'conversation',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '{"message_id": "8a7e6679-7425-40de-944b-e07fc1f90ae8", "kind": "text", "size_bytes": 17}'::jsonb,
        '2026-08-23T00:00:01Z'
    ),
    (
        'dddddddd-dddd-4ddd-8ddd-dddddddddddd',
        NULL,                                      -- system-initiated audit
        NULL,
        'system.environment.bootstrap',
        'environment',
        '7c9e6679-7425-40de-944b-e07fc1f90ae7',
        '{"reason": "test-fixture-load"}'::jsonb,
        '2026-08-23T00:00:00Z'
    )
ON CONFLICT (id) DO NOTHING;
