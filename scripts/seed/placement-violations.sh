#!/usr/bin/env bash
# placement-violations — `[accepts]` and `is:misplaced` exercise.
#
# What you get:
#   - schema.toml declaring two custom fields (scene, status) plus
#     the canonical alias catalog
#   - .dirmeta.toml at three levels: project root accepts inherit
#     mode, `assets/textures/` accepts only :image extensions,
#     `assets/models/` accepts only :3d extensions
#   - 6 files: 3 placed correctly, 3 misplaced (model in textures,
#     image in models, render in references)
#
# Use this seed to drive placement checks:
#   - `progest lint --format json` (placement violations populated)
#   - `progest search is:misplaced`
#   - `progest search is:violation`

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir placement-violations)}"

prepare_dir "${OUT}"
seed_init "${OUT}" placement-violations

step "Writing schema.toml (custom fields + extension compounds)"
seed_write "${OUT}" ".progest/schema.toml" <<'TOML'
schema_version = 1

[custom_fields.scene]
type = "int"

[custom_fields.status]
type = "enum"
values = ["wip", "review", "approved"]
TOML

step "Writing top-level .dirmeta.toml (no own accepts, just inherit policy)"
seed_write "${OUT}" "assets/.dirmeta.toml" <<'TOML'
schema_version = 1

# Children inherit by default — explicit subtree dirmeta below
# overrides where needed.
[accepts]
inherit = true
TOML

step "Writing assets/textures/.dirmeta.toml — accepts :image only"
seed_write "${OUT}" "assets/textures/.dirmeta.toml" <<'TOML'
schema_version = 1

[accepts]
exts = [":image"]
mode = "warn"
inherit = false
TOML

step "Writing assets/models/.dirmeta.toml — accepts :model builtin alias"
seed_write "${OUT}" "assets/models/.dirmeta.toml" <<'TOML'
schema_version = 1

[accepts]
exts = [":model"]
mode = "warn"
inherit = false
TOML

step "Writing correctly-placed files"
seed_touch "${OUT}" "assets/textures/wood.png"
seed_touch "${OUT}" "assets/textures/stone.jpg"
seed_touch "${OUT}" "assets/models/hero.fbx"

step "Writing misplaced files (intentional violations)"
# Model in textures dir.
seed_touch "${OUT}" "assets/textures/sneaky.fbx"
# Image in models dir.
seed_touch "${OUT}" "assets/models/concept.png"
# Random render outside any accept-aware dir.
seed_touch "${OUT}" "renders/final_render.exr"

seed_scan "${OUT}"
(cd "${OUT}" && progest lint >/dev/null) || true

note "expected misplaced: assets/textures/sneaky.fbx, assets/models/concept.png"
note "try:     progest search is:misplaced --format json | jq '.hits[].path'"

seed_done "${OUT}" placement-violations
