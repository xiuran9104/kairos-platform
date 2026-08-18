#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
compose=(docker compose -f "$repo_root/script/docker/docker-compose.yml" --profile red-team-labs)
action="${1:-up}"

case "$action" in
  up)
    "${compose[@]}" up -d juice-shop dvwa webgoat
    echo "Juice Shop: http://127.0.0.1:13000"
    echo "DVWA:       http://127.0.0.1:13001"
    echo "WebGoat:    http://127.0.0.1:13002/WebGoat"
    ;;
  stop)
    "${compose[@]}" stop juice-shop dvwa webgoat
    ;;
  status)
    "${compose[@]}" ps juice-shop dvwa webgoat
    ;;
  *)
    echo "usage: $0 {up|stop|status}" >&2
    exit 2
    ;;
esac
