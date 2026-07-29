#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output="$repo_root/sql/postgresql/0001_initial.sql"
temporary="${output}.tmp"
sanitized="${output}.sanitized.tmp"
database_url="${DATABASE_URL:-postgres://kairos_platform:kairos_platform@127.0.0.1:5432/kairos_platform}"

trap 'rm -f "$temporary" "$sanitized"' EXIT

pg_dump "$database_url" \
  --format=plain \
  --no-owner \
  --no-privileges \
  --exclude-table-data='pent_agent_sessions' \
  --exclude-table-data='pent_conversation_messages' \
  --exclude-table-data='pent_runs' \
  --exclude-table-data='pent_tool_calls' \
  --exclude-table-data='pent_approvals' \
  --exclude-table-data='pent_evidence' \
  --exclude-table-data='pent_findings' \
  --exclude-table-data='pent_http_traffic' \
  --exclude-table-data='pent_run_events' \
  --exclude-table-data='pent_run_instructions' \
  --exclude-table-data='scan_tasks' \
  --exclude-table-data='scan_port_details' \
  --exclude-table-data='system_oauth2_access_token' \
  --exclude-table-data='system_oauth2_refresh_token' \
  --exclude-table-data='system_login_log' \
  --exclude-table-data='system_operate_log' \
  --exclude-table-data='system_notify_message' \
  --file "$temporary"

test -s "$temporary"

# Keep baseline configuration rows while removing credential-bearing COPY fields.
awk '
BEGIN { FS = OFS = "\t"; in_copy = 0; sensitive_count = 0 }
/^COPY / {
  in_copy = 1
  sensitive_count = 0
  login_ip_column = 0
  login_date_column = 0
  sanitize_user_login = ($0 ~ /^COPY public[.]system_users /)
  header = $0
  sub(/^COPY [^(]+\(/, "", header)
  sub(/\) FROM stdin;$/, "", header)
  column_count = split(header, columns, /,[[:space:]]*/)
  for (i = 1; i <= column_count; i++) {
    name = columns[i]
    gsub(/^[[:space:]"]+|[[:space:]"]+$/, "", name)
    if (name ~ /^(api_key|api_secret|password|secret|access_key|secret_key|token)$/) {
      sensitive[++sensitive_count] = i
    }
    if (sanitize_user_login && name == "login_ip") {
      login_ip_column = i
    }
    if (sanitize_user_login && name == "login_date") {
      login_date_column = i
    }
  }
  print
  next
}
in_copy && $0 == "\\." {
  in_copy = 0
  delete sensitive
  print
  next
}
in_copy {
  for (i = 1; i <= sensitive_count; i++) {
    $sensitive[i] = ""
  }
  if (login_ip_column > 0) {
    $login_ip_column = ""
  }
  if (login_date_column > 0) {
    $login_date_column = "\\N"
  }
}
{ print }
' "$temporary" > "$sanitized"

mv "$sanitized" "$temporary"

if rg -n -i \
  '((^|[^[:alnum:]])sk-[a-z0-9_-]{16,}|authorization:[[:space:]]*bearer[[:space:]]+[a-z0-9._-]{12,}|password=[^[:space:]]{8,})' \
  "$temporary"; then
  echo "database bootstrap export rejected: possible secret detected" >&2
  exit 1
fi
mv "$temporary" "$output"
chmod 0644 "$output"
sha256sum "$output"
trap - EXIT
