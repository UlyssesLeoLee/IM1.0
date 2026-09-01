#!/bin/bash
# Init a local PG 18.6 cluster for B-1 migration testing (no destructive ops)
set -e
export PATH=/usr/lib/postgresql/18/bin:$PATH
PGDATA=/tmp/pg18-b1-$(date +%s)
echo "Using PGDATA=$PGDATA"
initdb -D "$PGDATA" --username=leo19 --auth=trust --encoding=UTF8 --locale=C 2>&1 | tail -5
pg_ctl -D "$PGDATA" -l /tmp/pg18-b1-$(date +%s).log -o "-p 5544 -k /tmp" start 2>&1
sleep 2
psql -h /tmp -p 5544 -U leo19 -d postgres -c "SELECT version();" 2>&1
echo "PGDATA=$PGDATA" > /tmp/pg18-b1.env
