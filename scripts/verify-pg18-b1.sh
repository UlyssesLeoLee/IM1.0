#!/bin/bash
# Verify B-1 migration results on PG 18.6
set -e
export PGPASSWORD=
PSQL="psql -h /tmp -p 5544 -U leo19 -d postgres"

echo "=== 1) Total tables in public schema ==="
$PSQL -t -c "SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE';"

echo "=== 2) Tables by name (alphabetical) ==="
$PSQL -c "SELECT table_name FROM information_schema.tables WHERE table_schema='public' AND table_type='BASE TABLE' ORDER BY table_name;"

echo "=== 3) Triggers ==="
$PSQL -c "SELECT event_object_table AS table_name, trigger_name, action_timing, event_manipulation FROM information_schema.triggers WHERE trigger_schema='public' ORDER BY event_object_table;"

echo "=== 4) Unique + Check constraints ==="
$PSQL -c "SELECT conrelid::regclass::text AS table_name, conname, contype, pg_get_constraintdef(oid) AS definition FROM pg_constraint WHERE connamespace='public'::regnamespace AND contype IN ('u','c') ORDER BY conrelid::regclass::text, contype, conname;"

echo "=== 5) Indexes ==="
$PSQL -c "SELECT schemaname, tablename, indexname FROM pg_indexes WHERE schemaname='public' ORDER BY tablename, indexname;"

echo "=== 6) Foreign keys ==="
$PSQL -c "SELECT conrelid::regclass::text AS from_table, conname, pg_get_constraintdef(oid) AS definition FROM pg_constraint WHERE contype='f' AND connamespace='public'::regnamespace ORDER BY conrelid::regclass::text;"

echo "=== 7) Insert smoke (tenant + game + env + user) ==="
$PSQL <<'SQL'
BEGIN;
INSERT INTO tenants(id, name) VALUES (gen_random_uuid(), 'B1-Verify-Studio') RETURNING id \gset
INSERT INTO games(id, tenant_id, name) VALUES (gen_random_uuid(), :'id', 'B1-Verify-Game') RETURNING id \gset game_
INSERT INTO environments(id, game_id, name) VALUES (gen_random_uuid(), :'game_id', 'test') RETURNING id \gset env_
INSERT INTO users(id, environment_id, kind) VALUES (gen_random_uuid(), :'env_id', 'guest') RETURNING id;
COMMIT;
SQL

echo "=== 8) Idempotency key UNIQUE NULLS NOT DISTINCT (PG 15+ feature) check ==="
$PSQL -c "SELECT conname, pg_get_constraintdef(oid) FROM pg_constraint WHERE conname='uniq_messages_idem';"

echo "=== 9) updated_at trigger function ==="
$PSQL -c "SELECT proname, prosrc FROM pg_proc WHERE proname='set_updated_at';"

echo "=== 10) pgcrypto extension ==="
$PSQL -c "SELECT extname, extversion FROM pg_extension WHERE extname='pgcrypto';"

echo "=== 11) pgsql version ==="
$PSQL -c "SELECT version();"
