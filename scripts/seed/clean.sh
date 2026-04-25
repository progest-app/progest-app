#!/usr/bin/env bash
# Wipe every seed output under `tmp/seed/`.
#
# Each per-pattern script removes its own output before regenerating,
# but you may want to clear everything at once (e.g. before pushing
# a branch, or to free disk space). This script does that.

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

ROOT_OUT="${1:-${REPO_ROOT}/tmp/seed}"

if [[ ! -d "${ROOT_OUT}" ]]; then
    note "${ROOT_OUT} doesn't exist — nothing to clean"
    exit 0
fi

step "Removing ${ROOT_OUT}"
rm -rf "${ROOT_OUT}"
note "done"
