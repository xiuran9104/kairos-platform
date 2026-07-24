#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
container="kairos-platform-storyboard-test"
port="${TEST_STORYBOARD_POSTGRES_PORT:-55433}"
cleanup() { docker rm -f "$container" >/dev/null 2>&1 || true; }
trap cleanup EXIT
cleanup
docker run -d --name "$container" \
  -e POSTGRES_USER=kairos_platform \
  -e POSTGRES_PASSWORD=kairos_platform \
  -e POSTGRES_DB=kairos_platform_test \
  -v "$repo_root/sql/postgresql/0001_initial.sql:/docker-entrypoint-initdb.d/0001_initial.sql:ro" \
  -p "$port:5432" postgres:18 >/dev/null
for _ in $(seq 1 30); do
  docker exec "$container" pg_isready -h 127.0.0.1 -p 5432 -U kairos_platform -d kairos_platform_test >/dev/null 2>&1 && break
  sleep 1
done
export TEST_DATABASE_URL="postgres://kairos_platform:kairos_platform@127.0.0.1:${port}/kairos_platform_test"
cargo test -p kairos-platform-toon-server storyboard_database_tests -- --ignored --nocapture
