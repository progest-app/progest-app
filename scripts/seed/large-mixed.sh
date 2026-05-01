#!/usr/bin/env bash
# large-mixed — 大規模混合プロジェクト（全要素を詰め込んだストレステスト）。
#
# What you get:
#   - ~250 files spanning VFX shots, game assets, 3DCG, documents
#   - Deep directory nesting (5+ levels)
#   - Multiple rules with overlapping scopes
#   - Sequences with drift candidates
#   - UDIM textures, versioned renders, shot hierarchies
#   - ~30 naming violations, ~15 placement violations
#   - ~20 tagged files across 8 different tags

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir large-mixed)}"

prepare_dir "${OUT}"
seed_init "${OUT}" large-mixed

# ── rules.toml ──────────────────────────────────────────────────────

step "Writing rules.toml (multi-scope)"
seed_write "${OUT}" ".progest/rules.toml" <<'TOML'
schema_version = 1

[[rules]]
id = "global-snake"
kind = "constraint"
applies_to = "./**"
mode = "warn"
charset = "ascii"
casing = "snake"
forbidden_chars = [" ", "　"]

[[rules]]
id = "shot-snake"
kind = "constraint"
applies_to = "./shots/**"
mode = "warn"
casing = "snake"

[[rules]]
id = "asset-naming"
kind = "constraint"
applies_to = "./assets/**"
mode = "warn"
casing = "snake"
TOML

# ── schema.toml ─────────────────────────────────────────────────────

step "Writing schema.toml"
seed_write "${OUT}" ".progest/schema.toml" <<'TOML'
schema_version = 1

[custom_fields.department]
type = "enum"
values = ["modeling", "texturing", "lighting", "compositing", "animation", "fx", "rigging"]

[custom_fields.priority]
type = "enum"
values = ["low", "medium", "high", "critical"]

[custom_fields.shot_number]
type = "int"
TOML

# ── .dirmeta.toml ───────────────────────────────────────────────────

step "Writing .dirmeta.toml hierarchy"

seed_write "${OUT}" "assets/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
inherit = true
TOML

seed_write "${OUT}" "assets/models/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":model", ".abc", ".usd", ".usdc", ".usda"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "assets/textures/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":image", ".tx", ".exr"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "shots/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [".exr", ".dpx", ".png"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "assets/audio/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":audio"]
mode = "warn"
inherit = false
TOML

# ── Shot hierarchy (VFX pipeline) ───────────────────────────────────

step "Writing VFX shot hierarchy (~60 files)"

for ep in ep01 ep02; do
    for scene in sc010 sc020 sc030; do
        for pass in beauty depth normal matte; do
            for frame in $(seq 1 3); do
                padded=$(printf '%04d' "${frame}")
                seed_touch "${OUT}" "shots/${ep}/${scene}/${scene}_${pass}_${padded}.exr"
            done
        done
    done
done

# Naming violations in shots
seed_touch "${OUT}" "shots/ep01/sc010/SC010_Beauty_0001.exr"
seed_touch "${OUT}" "shots/ep01/sc020/sc020 depth 0001.exr"

# ── 3D models (deep nesting) ───────────────────────────────────────

step "Writing 3D model assets (~40 files)"

for category in characters vehicles props environments; do
    for asset in alpha bravo charlie delta echo foxtrot; do
        seed_touch "${OUT}" "assets/models/${category}/${asset}/${asset}_high.fbx"
        seed_touch "${OUT}" "assets/models/${category}/${asset}/${asset}_low.fbx"
    done
done

# Naming violations
seed_touch "${OUT}" "assets/models/characters/alpha/Alpha High.fbx"
seed_touch "${OUT}" "assets/models/vehicles/bravo/BravoLow.fbx"

# Placement violations: textures in models dir
seed_touch "${OUT}" "assets/models/characters/alpha/alpha_diffuse.png"
seed_touch "${OUT}" "assets/models/vehicles/bravo/bravo_normal.jpg"

# ── Textures (UDIM + standard) ─────────────────────────────────────

step "Writing texture assets (~50 files)"

for asset in alpha bravo charlie delta echo foxtrot golf hotel; do
    for map in diffuse normal roughness metallic ao; do
        seed_touch "${OUT}" "assets/textures/${asset}/${asset}_${map}.exr"
    done
done

# UDIM sets for hero assets
for asset in alpha bravo; do
    for udim in 1001 1002 1003 1004; do
        seed_touch "${OUT}" "assets/textures/${asset}/${asset}_udim_${udim}.exr"
    done
done

# Naming violations in textures
seed_touch "${OUT}" "assets/textures/charlie/Charlie Diffuse.exr"
seed_touch "${OUT}" "assets/textures/delta/DeltaNormal.exr"
seed_touch "${OUT}" "assets/textures/echo/echo AO.exr"

# ── Audio assets ────────────────────────────────────────────────────

step "Writing audio assets (~20 files)"

for sfx in explosion_01 explosion_02 footstep_concrete footstep_metal \
           glass_break impact_heavy impact_light whoosh_fast whoosh_slow \
           ui_click ui_hover ui_confirm alarm_warning alarm_critical; do
    seed_touch "${OUT}" "assets/audio/sfx/${sfx}.wav"
done
for music in theme_main theme_action theme_calm ambient_rain ambient_wind; do
    seed_touch "${OUT}" "assets/audio/music/${music}.ogg"
done

# Placement violation: model in audio dir
seed_touch "${OUT}" "assets/audio/stray_model.fbx"

# Naming violations
seed_touch "${OUT}" "assets/audio/sfx/Laser Beam.wav"
seed_touch "${OUT}" "assets/audio/music/BossTheme.ogg"

# ── Compositing / plates ───────────────────────────────────────────

step "Writing comp plates (~25 files)"

for ep in ep01 ep02; do
    for scene in sc010 sc020; do
        for plate in bg fg overlay; do
            for frame in $(seq 1 4); do
                padded=$(printf '%04d' "${frame}")
                seed_touch "${OUT}" "comp/${ep}/${scene}/${scene}_${plate}_${padded}.exr"
            done
        done
    done
done

# ── Documents / references ─────────────────────────────────────────

step "Writing docs and references (~20 files)"

for doc in project_brief character_design_doc shot_list asset_tracker \
           color_script storyboard_v01 storyboard_v02 storyboard_v03 \
           schedule_q1 schedule_q2; do
    seed_touch "${OUT}" "docs/${doc}.pdf"
done
for ref in color_ref_01 color_ref_02 lighting_ref_day lighting_ref_night \
           material_ref_metal material_ref_wood material_ref_fabric \
           pose_ref_01 pose_ref_02 pose_ref_03; do
    seed_touch "${OUT}" "references/${ref}.jpg"
done

# ── Sequence drift candidates ──────────────────────────────────────

step "Writing sequence drift candidates"

# Canonical: underscore separator, 4-padded
for i in $(seq 1 6); do
    padded=$(printf '%04d' "${i}")
    seed_touch "${OUT}" "shots/ep01/sc030/sc030_beauty_${padded}.exr"
done

# Drift: same stem, different case
for i in $(seq 1 3); do
    padded=$(printf '%04d' "${i}")
    seed_touch "${OUT}" "shots/ep01/sc030/SC030_Beauty_${padded}.exr"
done

# Drift: different separator
for i in 1 2 3; do
    padded=$(printf '%04d' "${i}")
    seed_touch "${OUT}" "shots/ep01/sc030/sc030-beauty-${padded}.exr"
done

# ── Scan and tag ────────────────────────────────────────────────────

seed_scan "${OUT}"
(cd "${OUT}" && progest lint >/dev/null) || true

step "Tagging files (~20 tags)"
(
    cd "${OUT}"
    progest tag add wip assets/models/characters/alpha/alpha_high.fbx >/dev/null
    progest tag add wip assets/models/characters/bravo/bravo_high.fbx >/dev/null
    progest tag add wip assets/textures/alpha/alpha_diffuse.exr >/dev/null
    progest tag add review assets/models/characters/charlie/charlie_high.fbx >/dev/null
    progest tag add review assets/textures/charlie/charlie_diffuse.exr >/dev/null
    progest tag add approved assets/models/props/delta/delta_high.fbx >/dev/null
    progest tag add approved assets/models/props/delta/delta_low.fbx >/dev/null
    progest tag add hero assets/models/characters/alpha/alpha_high.fbx >/dev/null
    progest tag add hero assets/textures/alpha/alpha_diffuse.exr >/dev/null
    progest tag add final assets/audio/music/theme_main.ogg >/dev/null
    progest tag add placeholder assets/textures/hotel/hotel_diffuse.exr >/dev/null
    progest tag add placeholder assets/textures/golf/golf_diffuse.exr >/dev/null
    progest tag add retake shots/ep01/sc010/sc010_beauty_0001.exr >/dev/null
    progest tag add retake shots/ep01/sc020/sc020_beauty_0001.exr >/dev/null
    progest tag add reference references/color_ref_01.jpg >/dev/null
    progest tag add reference references/lighting_ref_day.jpg >/dev/null
    progest tag add outdated docs/schedule_q1.pdf >/dev/null
    progest tag add urgent assets/models/characters/alpha/alpha_high.fbx >/dev/null
    progest tag add urgent shots/ep01/sc010/sc010_beauty_0001.exr >/dev/null
    progest tag add note docs/project_brief.pdf >/dev/null
)

seed_done "${OUT}" large-mixed
