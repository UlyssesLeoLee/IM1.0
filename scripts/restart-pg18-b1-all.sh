#!/bin/bash
# Re-init PG 18.6 on 0.0.0.0:5544 (allow Windows host to reach via WSL eth0 IP)
# Used by cargo test (Windows side) to run integration tests
set -e
export PATH=/usr/lib/postgresql/18/bin:$PATH

# 拿上次的 PGDATA,或起新的
if [ -f /tmp/pg18-b1.env ]; then
  source /tmp/pg18-b1.env
fi

if [ -z "$PGDATA" ] || [ ! -d "$PGDATA" ]; then
  PGDATA=/tmp/pg18-b1-$(date +%s)
  initdb -D "$PGDATA" --username=leo19 --auth=trust --encoding=UTF8 --locale=C 2>&1 | tail -3
  echo "PGDATA=$PGDATA" > /tmp/pg18-b1.env
fi

# 停掉旧实例(可能绑在 127.0.0.1)
pg_ctl -D "$PGDATA" stop -m fast 2>&1 || true
sleep 1

# 改 listen_addresses 0.0.0.0
PGCONF="$PGDATA/postgresql.conf"
if ! grep -q "^listen_addresses = '*'" "$PGCONF"; then
  echo "listen_addresses = '*'" >> "$PGCONF"
fi
# 也放开 pg_hba.conf 让 leo19 从任意 host 进来(测试用)
PGHBA="$PGDATA/pg_hba.conf"
if ! grep -q "host all leo19 0.0.0.0/0 trust" "$PGHBA"; then
  echo "host all leo19 0.0.0.0/0 trust" >> "$PGHBA"
fi

pg_ctl -D "$PGDATA" -l /tmp/pg18-b1.log -o "-p 5544 -k /tmp" start 2>&1
sleep 2
psql -h 127.0.0.1 -p 5544 -U leo19 -d postgres -c "SELECT version();" 2>&1 | head -3

# 给 Windows 端输出可达 URL
WSL_IP=$(ip -4 addr show eth0 | awk '/inet /{print $2}' | cut -d/ -f1)
echo ""
echo "Windows-side DATABASE_URL: postgres://leo19@${WSL_IP}:5544/postgres"
