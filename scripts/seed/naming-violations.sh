#!/usr/bin/env bash
# naming-violations — `progest clean` / `progest rename` exercise.
#
# What you get:
#   - 8 files with mechanical naming issues:
#     * spaces in basename
#     * Pascal / Mixed case
#     * trailing OS copy suffix (`foo (2).psd` etc.)
#     * embedded CJK characters
#   - rules.toml with snake_case + ASCII constraints so `progest
#     lint` flags every violator
#
# Use this seed to drive the cleanup pipeline:
#   - `progest clean --case=snake --strip-suffix --strip-cjk`
#   - `progest clean --apply --fill-mode=placeholder --placeholder=_`
#   - `progest rename --case=snake --apply`

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir naming-violations)}"

prepare_dir "${OUT}"
seed_init "${OUT}" naming-violations

step "Writing rules.toml (snake-case + ASCII enforcement)"
seed_write "${OUT}" ".progest/rules.toml" <<'TOML'
schema_version = 1

[[rules]]
id = "ascii-snake"
kind = "constraint"
applies_to = "./assets/**"
mode = "warn"
charset = "ascii"
casing = "snake"
forbidden_chars = [" ", "　"]
TOML

step "Writing files with mechanical naming issues"
# Spaces.
seed_touch "${OUT}" "assets/Forest Night.psd"
seed_touch "${OUT}" "assets/Closeup Of Hero.tif"
# Pascal case.
seed_touch "${OUT}" "assets/HeroCharacter.psd"
seed_touch "${OUT}" "assets/MountainDawn.psd"
# OS copy suffix.
seed_touch "${OUT}" "assets/forest (2).psd"
seed_touch "${OUT}" "assets/forest - Copy.psd"
# Embedded CJK.
seed_touch "${OUT}" "assets/森_night.psd"
seed_touch "${OUT}" "assets/夕暮れ_hero.psd"

step "Writing one already-clean file as a control"
seed_touch "${OUT}" "assets/already_clean.psd"

seed_scan "${OUT}"
(cd "${OUT}" && progest lint >/dev/null) || true

note "try: progest clean --case=snake --strip-suffix --strip-cjk"
note "    or: progest clean --apply --fill-mode=placeholder --placeholder=_"

seed_done "${OUT}" naming-violations
