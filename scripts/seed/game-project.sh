#!/usr/bin/env bash
# game-project — ゲーム開発プロジェクト（アセットタイプ別構成）。
#
# What you get:
#   - ~150 files in a game asset pipeline layout:
#     characters/ environments/ ui/ audio/ scripts/ prefabs/
#   - rules.toml with snake_case + ASCII enforcement
#   - schema.toml with custom fields (lod_level, asset_status)
#   - .dirmeta.toml with accepts constraints per directory
#   - Naming violations, placement violations, tagged files
#   - Multiple file types: fbx, png, jpg, wav, ogg, cs, prefab, mat, anim

set -euo pipefail
SEED_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_lib.sh
source "${SEED_DIR}/_lib.sh"

OUT="${1:-$(default_out_dir game-project)}"

prepare_dir "${OUT}"
seed_init "${OUT}" game-project

# ── rules.toml ──────────────────────────────────────────────────────

step "Writing rules.toml"
seed_write "${OUT}" ".progest/rules.toml" <<'TOML'
schema_version = 1

[[rules]]
id = "asset-snake"
kind = "constraint"
applies_to = "./assets/**"
mode = "warn"
charset = "ascii"
casing = "snake"
forbidden_chars = [" ", "　"]

[[rules]]
id = "script-snake"
kind = "constraint"
applies_to = "./scripts/**"
mode = "warn"
casing = "snake"
TOML

# ── schema.toml ─────────────────────────────────────────────────────

step "Writing schema.toml"
seed_write "${OUT}" ".progest/schema.toml" <<'TOML'
schema_version = 1

[custom_fields.lod_level]
type = "int"

[custom_fields.asset_status]
type = "enum"
values = ["wip", "review", "approved", "deprecated"]
TOML

# ── .dirmeta.toml ───────────────────────────────────────────────────

step "Writing .dirmeta.toml constraints"

seed_write "${OUT}" "assets/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
inherit = true
TOML

seed_write "${OUT}" "assets/characters/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":model", ":image", ".mat", ".anim"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "assets/environments/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":model", ":image", ".mat", ".prefab"]
mode = "warn"
inherit = false
TOML

seed_write "${OUT}" "assets/ui/.dirmeta.toml" <<'TOML'
schema_version = 1
[accepts]
exts = [":image", ".svg"]
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

# ── Characters ──────────────────────────────────────────────────────

step "Writing character assets (~40 files)"

for char in knight mage archer healer rogue; do
    for part in body head weapon armor; do
        seed_touch "${OUT}" "assets/characters/${char}/${char}_${part}.fbx"
    done
    for tex in diffuse normal roughness; do
        seed_touch "${OUT}" "assets/characters/${char}/textures/${char}_${part}_${tex}.png"
    done
    seed_touch "${OUT}" "assets/characters/${char}/${char}_idle.anim"
    seed_touch "${OUT}" "assets/characters/${char}/${char}_run.anim"
    seed_touch "${OUT}" "assets/characters/${char}/${char}_attack.anim"
    seed_touch "${OUT}" "assets/characters/${char}/${char}.mat"
done

# Naming violations in characters
seed_touch "${OUT}" "assets/characters/knight/Knight_Shield.fbx"
seed_touch "${OUT}" "assets/characters/mage/Mage Staff.fbx"
seed_touch "${OUT}" "assets/characters/archer/ArrowQuiver.fbx"

# ── Environments ────────────────────────────────────────────────────

step "Writing environment assets (~45 files)"

for env in forest castle dungeon village desert; do
    seed_touch "${OUT}" "assets/environments/${env}/${env}_ground.fbx"
    seed_touch "${OUT}" "assets/environments/${env}/${env}_walls.fbx"
    seed_touch "${OUT}" "assets/environments/${env}/${env}_props.fbx"
    seed_touch "${OUT}" "assets/environments/${env}/${env}_skybox.fbx"
    seed_touch "${OUT}" "assets/environments/${env}/${env}.prefab"
    for tex in diffuse normal ao; do
        seed_touch "${OUT}" "assets/environments/${env}/textures/${env}_${tex}.png"
    done
    seed_touch "${OUT}" "assets/environments/${env}/${env}.mat"
done

# Placement violations: model files in wrong directories
seed_touch "${OUT}" "assets/ui/sneaky_model.fbx"
seed_touch "${OUT}" "assets/audio/misplaced_texture.png"

# ── UI ──────────────────────────────────────────────────────────────

step "Writing UI assets (~25 files)"

for screen in main_menu inventory hud settings game_over; do
    seed_touch "${OUT}" "assets/ui/${screen}/${screen}_bg.png"
    seed_touch "${OUT}" "assets/ui/${screen}/${screen}_frame.png"
    seed_touch "${OUT}" "assets/ui/${screen}/${screen}_icons.png"
done
for icon in health mana stamina gold xp shield potion sword bow staff; do
    seed_touch "${OUT}" "assets/ui/icons/icon_${icon}.png"
done

# ── Audio ───────────────────────────────────────────────────────────

step "Writing audio assets (~30 files)"

for sfx in sword_hit arrow_fire spell_cast footstep_grass footstep_stone \
           door_open chest_open coin_pickup level_up death; do
    seed_touch "${OUT}" "assets/audio/sfx/${sfx}.wav"
done
for music in main_theme battle_theme village_theme dungeon_theme boss_theme \
             victory_fanfare game_over_theme; do
    seed_touch "${OUT}" "assets/audio/music/${music}.ogg"
done
for amb in forest_ambience cave_drip wind_howl rain_light campfire; do
    seed_touch "${OUT}" "assets/audio/ambient/${amb}.ogg"
done

# Naming violations in audio
seed_touch "${OUT}" "assets/audio/sfx/Magic Explosion.wav"
seed_touch "${OUT}" "assets/audio/sfx/FireBall_Impact.wav"

# ── Scripts ─────────────────────────────────────────────────────────

step "Writing script files (~15 files)"

for script in player_controller enemy_ai inventory_manager quest_system \
              dialogue_handler save_manager ui_manager audio_manager \
              combat_system pathfinding level_loader particle_system \
              camera_controller input_handler network_sync; do
    seed_touch "${OUT}" "scripts/${script}.cs"
done

# ── Prefabs ─────────────────────────────────────────────────────────

step "Writing prefab files (~10 files)"

for prefab in player_character enemy_goblin enemy_skeleton enemy_dragon \
              treasure_chest health_potion mana_potion npc_merchant \
              campfire torch; do
    seed_touch "${OUT}" "assets/prefabs/${prefab}.prefab"
done

# ── Scan and tag ────────────────────────────────────────────────────

seed_scan "${OUT}"
(cd "${OUT}" && progest lint >/dev/null) || true

step "Tagging files"
(
    cd "${OUT}"
    progest tag add wip assets/characters/knight/knight_body.fbx >/dev/null
    progest tag add wip assets/characters/mage/mage_body.fbx >/dev/null
    progest tag add review assets/environments/forest/forest_ground.fbx >/dev/null
    progest tag add review assets/environments/castle/castle_walls.fbx >/dev/null
    progest tag add approved assets/ui/main_menu/main_menu_bg.png >/dev/null
    progest tag add approved assets/audio/music/main_theme.ogg >/dev/null
    progest tag add hero assets/characters/knight/knight_body.fbx >/dev/null
    progest tag add boss assets/characters/rogue/rogue_body.fbx >/dev/null
    progest tag add placeholder assets/environments/desert/desert_ground.fbx >/dev/null
    progest tag add final assets/audio/music/main_theme.ogg >/dev/null
)

seed_done "${OUT}" game-project
