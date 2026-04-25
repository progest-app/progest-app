#!/usr/bin/env bash
# sequences — sequence detection / drift / `--sequence-stem` rename
# exercise.
#
# What you get:
#   - `assets/anim/frame_0001.exr` … `frame_0010.exr` (10-member
#     canonical sequence, 4-padded)
#   - `assets/anim2/Render_001.exr` … `Render_005.exr` (Pascal-case
#     stem, 3-padded — drift candidate)
#   - `assets/anim3/render-1.exr` … `render-3.exr` (kebab + no
#     padding — separator/padding drift)
#   - 2 singleton stragglers (`promo_hero.exr`, `concept.png`)
#
# Use this seed to drive sequence-aware tooling:
#   - `progest clean` (sequence-aware preview)
#   - `progest rename --sequence-stem hero`
#   - `progest lint` (drift category)
#   - `progest search is:violation`

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir sequences)}"

prepare_dir "${OUT}"
seed_init "${OUT}" sequences

step "Writing canonical 10-member sequence (4-padded, underscore)"
# All under the SAME parent directory `assets/anim/`. Sequence
# drift detection groups by (parent, normalized-stem, ext), so the
# adjacent sequences below will fight for which separator/padding
# is canonical.
for i in $(seq 1 10); do
    padded=$(printf '%04d' "${i}")
    seed_touch "${OUT}" "assets/anim/frame_${padded}.exr"
done

step "Writing Pascal-case stem sequence at the same parent (drift target)"
# Same parent + normalized stem (`frame` ↔ `Frame` after lowercasing)
# but stem-case drift.
for i in $(seq 1 5); do
    padded=$(printf '%04d' "${i}")
    seed_touch "${OUT}" "assets/anim/Frame_${padded}.exr"
done

step "Writing kebab-separator + 3-pad sequence in another parent (clean canonical)"
# Different parent → not a drift candidate against the underscore
# group above. Useful as a control for the drift detection logic.
for i in 1 2 3; do
    padded=$(printf '%03d' "${i}")
    seed_touch "${OUT}" "renders/render-${padded}.exr"
done

step "Writing singleton stragglers"
seed_touch "${OUT}" "assets/anim/promo_hero.exr"
seed_touch "${OUT}" "concept.png"

seed_scan "${OUT}"

step "progest clean --format json (sequence-aware preview)"
note "see how each sequence is grouped under a stable seq-{uuid}"
(cd "${OUT}" && progest clean --format json | head -c 2000) || true
echo

note "try:  progest rename --sequence-stem shot01 assets/anim/"
note "try:  progest lint --format json | jq '.sequence'"

seed_done "${OUT}" sequences
