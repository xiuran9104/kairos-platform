#!/usr/bin/env bash
set -euo pipefail

readonly repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
readonly image="${PENTA_TOOLKIT_IMAGE:-kairos-platform/pent-toolkit:kali-2026.1}"

docker build \
  --file "${repository_root}/script/docker/pent-toolkit/Dockerfile" \
  --tag "${image}" \
  "${repository_root}"

docker run --rm --network none "${image}" /usr/local/bin/kairos-toolkit-doctor
echo "Built ${image}"
