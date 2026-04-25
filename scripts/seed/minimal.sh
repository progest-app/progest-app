#!/usr/bin/env bash
# minimal — small clean project, no rules, no violations.
#
# What you get:
#   - 5 PSD files + 2 TIFs + 2 reference images in a flat layout
#   - 2 tags applied via `progest tag add` so search results have
#     something interesting to inspect
#   - no `rules.toml`, `schema.toml`, or `.dirmeta.toml`
#
# Use this seed when you just want `progest search` / `progest tag`
# / `progest view` smoke surfaces to play with.

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir minimal)}"

prepare_dir "${OUT}"
seed_init "${OUT}" minimal

step "Writing flat asset tree"
for f in alpha bravo charlie delta echo; do
    seed_touch "${OUT}" "assets/${f}.psd"
done
seed_touch "${OUT}" "assets/wide_shot.tif"
seed_touch "${OUT}" "assets/closeup.tif"
seed_touch "${OUT}" "references/colorboard.jpg"
seed_touch "${OUT}" "references/storyboard.png"

seed_scan "${OUT}"

step "Tagging two files"
(
    cd "${OUT}"
    progest tag add wip assets/alpha.psd >/dev/null
    progest tag add review assets/bravo.psd >/dev/null
)

note "Tagged: assets/alpha.psd (wip), assets/bravo.psd (review)"

seed_done "${OUT}" minimal
