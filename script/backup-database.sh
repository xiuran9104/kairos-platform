#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
backup_dir="${DATABASE_BACKUP_DIR:-$repo_root/backups/postgresql}"
database_url="${DATABASE_URL:-postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform}"
timestamp="$(date +'%Y%m%d_%H%M%S')"
output="$backup_dir/kairos_platform_${timestamp}.dump"
temporary="${output}.tmp"

command -v pg_dump >/dev/null 2>&1 || {
  echo "pg_dump is required" >&2
  exit 1
}

mkdir -p "$backup_dir"
chmod 0700 "$backup_dir"

if [[ -e "$output" ]]; then
  echo "backup already exists: $output" >&2
  exit 1
fi

cleanup() {
  rm -f "$temporary"
}
trap cleanup EXIT

pg_dump "$database_url" \
  --format=custom \
  --no-owner \
  --no-privileges \
  --file "$temporary"

test -s "$temporary"
chmod 0600 "$temporary"
mv "$temporary" "$output"
trap - EXIT

echo "$output"
