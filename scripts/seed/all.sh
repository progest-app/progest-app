#!/usr/bin/env bash
# Run every seed pattern, each into its own subdirectory under
# `tmp/seed/`. Useful for quickly populating a sandbox with the full
# range of fixture types before diving into manual testing.

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

PATTERNS=(minimal shots-pipeline naming-violations placement-violations sequences)

ROOT_OUT="${1:-${REPO_ROOT}/tmp/seed}"
step "Seeding into ${ROOT_OUT}"

for p in "${PATTERNS[@]}"; do
    "${SEED_DIR}/${p}.sh" "${ROOT_OUT}/${p}"
    printf '\n'
done

step "All seeds complete:"
for p in "${PATTERNS[@]}"; do
    printf '   %s/%s\n' "${ROOT_OUT}" "${p}"
done
