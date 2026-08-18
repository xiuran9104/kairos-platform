#!/usr/bin/env bash
set -euo pipefail

base_url="${KAIROS_BASE_URL:-http://127.0.0.1:8080}"
username="${KAIROS_USERNAME:-admin}"
password="${KAIROS_PASSWORD:-Admin#123456}"
target="${RED_TEAM_TARGET:-http://127.0.0.1:13000}"
wait_for_completion="${RED_TEAM_WAIT_FOR_COMPLETION:-true}"
timeout_seconds="${RED_TEAM_TIMEOUT_SECONDS:-900}"
min_findings="${RED_TEAM_MIN_FINDINGS:-1}"

curl -fsS "$target" >/dev/null
token="$(curl -fsS -X POST "$base_url/system/auth/login" \
  -H 'Content-Type: application/json' \
  -d "{\"username\":\"$username\",\"password\":\"$password\"}" | jq -er '.data.access_token')"
auth="Authorization: Bearer $token"
expires_at="$(date -u -d '+2 hours' '+%Y-%m-%dT%H:%M:%SZ')"

project_id="$(curl -fsS -X POST "$base_url/pent/projects" \
  -H "$auth" -H 'Content-Type: application/json' \
  -d "{\"name\":\"Red Team Lab $(date +%s)\",\"asset_scope\":[\"$target\"],\"engagement_mode\":\"red_team\",\"rules_of_engagement\":{\"environment\":\"local_lab\",\"destructive_actions\":false}}" | jq -er '.data.id')"

curl -fsS -X PUT "$base_url/pent/projects/$project_id/authorization" \
  -H "$auth" -H 'Content-Type: application/json' \
  -d "{\"confirmed\":true,\"expires_at\":\"$expires_at\",\"rules_of_engagement\":{\"environment\":\"local_lab\",\"destructive_actions\":false}}" >/dev/null

task_id="$(curl -fsS -X POST "$base_url/pent/tasks" \
  -H "$auth" -H 'Content-Type: application/json' \
  -d "{\"project_id\":\"$project_id\",\"name\":\"Juice Shop 连续行动回归\",\"description\":\"在授权靶场完成指纹、枚举、验证并记录证据\",\"priority\":\"high\"}" | jq -er '.data.id')"
run_id="$(curl -fsS -X POST "$base_url/pent/runs" \
  -H "$auth" -H 'Content-Type: application/json' \
  -d "{\"task_id\":\"$task_id\"}" | jq -er '.data.id')"

echo "project_id=$project_id"
echo "task_id=$task_id"
echo "run_id=$run_id"
echo "Monitor: http://127.0.0.1:5666/pent/runs?id=$run_id"

if [[ "$wait_for_completion" != "true" ]]; then
  exit 0
fi

deadline="$(( $(date +%s) + timeout_seconds ))"
while true; do
  run_json="$(curl -fsS "$base_url/pent/runs/$run_id" -H "$auth")"
  status="$(jq -er '.data.status' <<<"$run_json")"
  case "$status" in
    completed|failed|blocked|cancelled)
      break
      ;;
  esac
  if (( $(date +%s) >= deadline )); then
    echo "red-team regression timed out after ${timeout_seconds}s (status=$status)" >&2
    exit 1
  fi
  sleep 2
done

if [[ "$status" != "completed" ]]; then
  failure="$(jq -r '.data.failure_message // "unknown failure"' <<<"$run_json")"
  echo "red-team regression failed: status=$status, error=$failure" >&2
  exit 1
fi

findings_json="$(curl -fsS "$base_url/pent/runs/$run_id/findings" -H "$auth")"
finding_count="$(jq -er 'if (.data | type) == "array" then .data | length else .data.items | length end' <<<"$findings_json")"
if (( finding_count < min_findings )); then
  echo "red-team regression found $finding_count findings; expected at least $min_findings" >&2
  exit 1
fi
echo "status=$status findings=$finding_count"
