#!/usr/bin/env bash
# cg-project — 3DCG プロジェクト（シーン + テクスチャ + レンダー出力）。
#
# What you get:
#   - ~180 files across scenes/ textures/ renders/ references/ cache/
#   - Shot-based render outputs with versioned sequences
#   - UDIM texture sets (1001–1004) for multiple assets
#   - rules.toml with snake_case + shot naming template
#   - schema.toml with render_pass, frame_range fields
#   - Naming violations, placement violations, sequences, tags

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir cg-project)}"

prepare_dir "${OUT}"
seed_init "${OUT}" cg-project

# ── rules.toml ──────────────────────────────────────────────────────

step "Writing rules.toml"
seed_write "${OUT}" ".progest/rules.toml" <<'TOML'
schema_version = 1

[[rules]]
id = "asset-snake"
kind = "constraint"
applies_to = "./**"
mode = "warn"
charset = "ascii"
casing = "snake"
forbidden_chars = [" ", "　"]

[[rules]]
id = "render-snake"
kind = "constraint"
applies_to = "./renders/**"
mode = "warn"
casing = "snake"
TOML

# ── schema.toml ─────────────────────────────────────────────────────

step "Writing schema.toml"
seed_write "${OUT}" ".progest/schema.toml" <<'TOML'
schema_version = 1

[custom_fields.render_pass]
type = "enum"
values = ["beauty", "diffuse", "specular", "depth", "normal", "ao", "shadow", "matte"]

[custom_fields.frame_range]
type = "string"
TOML

# ── .dirmeta.toml ───────────────────────────────────────────────────

step "Writing .dirmeta.toml constraints"

seed_write "${OUT}" "scenes/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [".blend", ".ma", ".mb", ".hip", ".c4d", ".max"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "textures/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":image", ".tx", ".rat"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "renders/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [".exr", ".dpx", ".png", ".jpg"]
mode = "warn"
inherit = false
TOML

# ── Scene files ─────────────────────────────────────────────────────

step "Writing scene files (~20 files)"

for scene in exterior_city interior_lab forest_chase rooftop_fight underwater; do
    seed_touch "${OUT}" "scenes/${scene}/${scene}.blend"
    seed_touch "${OUT}" "scenes/${scene}/${scene}_layout.blend"
    seed_touch "${OUT}" "scenes/${scene}/${scene}_lighting.blend"
    seed_touch "${OUT}" "scenes/${scene}/${scene}_anim.blend"
done

# ── Textures (UDIM sets) ───────────────────────────────────────────

step "Writing texture assets with UDIM sets (~60 files)"

for asset in hero_body hero_head villain_body villain_head \
             vehicle_main vehicle_detail building_facade building_interior \
             ground_dirt ground_grass ground_cobble ground_sand; do
    for udim in 1001 1002 1003 1004; do
        seed_touch "${OUT}" "textures/${asset}/${asset}_diffuse_${udim}.exr"
    done
    seed_touch "${OUT}" "textures/${asset}/${asset}_normal.exr"
    seed_touch "${OUT}" "textures/${asset}/${asset}_roughness.exr"
done

# Naming violations in textures
seed_touch "${OUT}" "textures/hero_body/Hero Body Spec.exr"
seed_touch "${OUT}" "textures/villain_head/VillainHead_AO.exr"

# ── Render outputs (sequences) ─────────────────────────────────────

step "Writing render output sequences (~70 files)"

for scene in exterior_city interior_lab forest_chase; do
    for pass in beauty diffuse depth normal; do
        for frame in $(seq 1 5); do
            padded=$(printf '%04d' "${frame}")
            seed_touch "${OUT}" "renders/${scene}/${scene}_${pass}_${padded}.exr"
        done
    done
done

# Extra long sequence for hero shot
for frame in $(seq 1 15); do
    padded=$(printf '%04d' "${frame}")
    seed_touch "${OUT}" "renders/rooftop_fight/rooftop_fight_beauty_${padded}.exr"
done

# Naming violation in renders
seed_touch "${OUT}" "renders/exterior_city/Exterior City_beauty_0001.exr"

# ── References ──────────────────────────────────────────────────────

step "Writing reference files (~15 files)"

for ref in color_palette mood_board_01 mood_board_02 storyboard_p01 \
           storyboard_p02 concept_hero concept_villain concept_vehicle \
           concept_environment location_photo_01 location_photo_02 \
           location_photo_03 lighting_ref_01 lighting_ref_02 camera_notes; do
    seed_touch "${OUT}" "references/${ref}.jpg"
done

# ── Cache / temp (placement violations) ────────────────────────────

step "Writing cache files (misplaced)"

seed_touch "${OUT}" "scenes/exterior_city/exterior_city_cache.abc"
seed_touch "${OUT}" "scenes/interior_lab/interior_lab_cache.vdb"
seed_touch "${OUT}" "textures/hero_body/hero_body.psd"

# Placement violation: scene file in textures
seed_touch "${OUT}" "textures/stray_scene.blend"
# Placement violation: render in scenes
seed_touch "${OUT}" "scenes/leaked_render.exr"

# ── Scan and tag ────────────────────────────────────────────────────

seed_scan "${OUT}"
(cd "${OUT}" && progest lint >/dev/null) || true

step "Tagging files"
(
    cd "${OUT}"
    progest tag add wip scenes/exterior_city/exterior_city.blend >/dev/null
    progest tag add wip scenes/interior_lab/interior_lab.blend >/dev/null
    progest tag add approved scenes/forest_chase/forest_chase.blend >/dev/null
    progest tag add review scenes/rooftop_fight/rooftop_fight.blend >/dev/null
    progest tag add hero textures/hero_body/hero_body_diffuse_1001.exr >/dev/null
    progest tag add hero textures/hero_head/hero_head_diffuse_1001.exr >/dev/null
    progest tag add final renders/rooftop_fight/rooftop_fight_beauty_0001.exr >/dev/null
    progest tag add retake renders/exterior_city/exterior_city_beauty_0001.exr >/dev/null
    progest tag add reference references/color_palette.jpg >/dev/null
    progest tag add reference references/mood_board_01.jpg >/dev/null
    progest tag add outdated references/lighting_ref_01.jpg >/dev/null
)

seed_done "${OUT}" cg-project
