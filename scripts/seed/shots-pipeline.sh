#!/usr/bin/env bash
# shots-pipeline —映像/VFX 風 shot/cut 命名のプロジェクト。
#
# What you get:
#   - `assets/shots/<chXXX>/<chXXX_NNN>_<role>_v<NN>.psd` の階層
#   - rules.toml に shot 命名テンプレートが入っている
#   - 違反ファイル 2 件 (大文字 / version 桁数違反) と
#     合格ファイル 4 件 が混在
#
# Use this seed for end-to-end naming-rule testing:
#   - `progest lint` / `progest lint --format json`
#   - `progest search is:violation`
#   - `progest clean --case=snake`

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir shots-pipeline)}"

prepare_dir "${OUT}"
seed_init "${OUT}" shots-pipeline

step "Writing rules.toml"
seed_write "${OUT}" ".progest/rules.toml" <<'TOML'
schema_version = 1

# Shot files must follow `chXXX_NNN_<role>_vNN.<ext>` (lowercase
# snake, 3-digit shot, 2-digit version).
[[rules]]
id = "shot-template"
kind = "template"
applies_to = "./assets/shots/**/*.psd"
mode = "warn"
template = "{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}"

# Snake-case constraint catches uppercase prefixes the template's
# open-ended `{prefix}` would otherwise accept.
[[rules]]
id = "shot-snake"
kind = "constraint"
applies_to = "./assets/shots/**/*.psd"
mode = "warn"
casing = "snake"

# Belt-and-braces: filenames anywhere under assets/ must be
# ASCII-only and free of spaces.
[[rules]]
id = "ascii-only"
kind = "constraint"
applies_to = "./assets/**"
mode = "warn"
charset = "ascii"
forbidden_chars = [" ", "　"]
TOML

step "Writing valid shot files"
for shot in 001 002 003; do
    seed_touch "${OUT}" "assets/shots/ch010/ch010_${shot}_bg_v01.psd"
done
seed_touch "${OUT}" "assets/shots/ch010/ch010_004_fg_v02.psd"

step "Writing intentionally violating files"
# Uppercase shot id — fails template (lowercase only).
seed_touch "${OUT}" "assets/shots/ch020/CH020_001_bg_v01.psd"
# Version with single digit — fails 2-digit padding.
seed_touch "${OUT}" "assets/shots/ch020/ch020_002_bg_v3.psd"

step "References (out of rules scope)"
seed_touch "${OUT}" "references/storyboard.pdf"

seed_scan "${OUT}"

step "progest lint (populates violations table)"
(cd "${OUT}" && progest lint >/dev/null) || true

note "expected: 2 naming warnings under assets/shots/ch020/"
note "try:     progest search is:violation --format json | jq '.hits[].path'"

seed_done "${OUT}" shots-pipeline
