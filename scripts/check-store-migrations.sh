#!/usr/bin/env bash
# Apply NXT store migrations to a local throwaway database and run the
# rolled-back SQL contract. Refuses any host that is not loopback and any
# database name that looks like production. Does not use Supabase credentials.
set -euo pipefail

url="${DATABASE_URL:-}"
if [[ -z "$url" ]]; then
  echo "DATABASE_URL is required and must point at a local throwaway database" >&2
  exit 2
fi
python3 - "$url" <<'PY'
import sys
from urllib.parse import urlparse
parsed = urlparse(sys.argv[1])
host = (parsed.hostname or "").lower()
name = (parsed.path or "").lstrip("/").lower()
if host not in {"localhost", "127.0.0.1", "::1"}:
    raise SystemExit("refusing non-loopback database host")
if not name or any(token in name for token in ("prod", "production", "live")):
    raise SystemExit("refusing a database name that looks like production")
print(f"local database check accepted for {name}")
PY

root="$(cd "$(dirname "$0")/.." && pwd)"
mapfile -t migrations < <(find "$root/supabase/migrations" -maxdepth 1 -type f -name '*.sql' | sort)
expected=(
  "20261025051013_nxt_store.sql"
  "20261026033000_nxt_backup_reader.sql"
  "20261029140000_company_access.sql"
)
if [[ "${#migrations[@]}" -ne "${#expected[@]}" ]]; then
  echo "expected exactly ${#expected[@]} store migrations, found ${#migrations[@]}" >&2
  exit 2
fi
for index in "${!expected[@]}"; do
  base="$(basename "${migrations[$index]}")"
  if [[ "$base" != "${expected[$index]}" ]]; then
    echo "migration order mismatch: $base" >&2
    exit 2
  fi
done
if [[ "$(sha256sum "${migrations[@]}" | awk '{print $1}' | sort | uniq -d | wc -l)" -ne 0 ]]; then
  echo "duplicate migration contents" >&2
  exit 2
fi

psql "$url" -v ON_ERROR_STOP=1 <<'SQL'
do $roles$
begin
  if not exists (select 1 from pg_roles where rolname = 'anon') then create role anon nologin; end if;
  if not exists (select 1 from pg_roles where rolname = 'authenticated') then create role authenticated nologin; end if;
  if not exists (select 1 from pg_roles where rolname = 'service_role') then create role service_role nologin; end if;
end
$roles$;
SQL

for migration in "${migrations[@]}"; do
  echo "applying $(basename "$migration") to local throwaway database"
  psql "$url" -v ON_ERROR_STOP=1 -f "$migration"
done
psql "$url" -v ON_ERROR_STOP=1 -f "$root/supabase/tests/store_contract.sql"
psql "$url" -v ON_ERROR_STOP=1 -f "$root/supabase/tests/company_access_contract.sql"
echo "local store migration contract passed; no production database was used"
