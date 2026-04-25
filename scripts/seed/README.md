# Seed scripts

Throwaway test-project generators. Each script materializes a self-contained Progest project (with `.progest/`, sample files, and rule / accepts configs as needed) so you can poke at CLI behaviour without hand-crafting fixtures.

All scripts are idempotent: re-running wipes the previous output before recreating.

## Quick start

```sh
# Build the CLI once (the seeds shell out to `progest`).
cargo build -p progest-cli

# Generate one pattern.
scripts/seed/minimal.sh

# Or generate all five patterns at once.
scripts/seed/all.sh

# Wipe every seed output.
scripts/seed/clean.sh
```

The default output directory is `tmp/seed/<pattern>/` at the repository root; pass an explicit path as the first arg to override:

```sh
scripts/seed/shots-pipeline.sh /tmp/my-pipeline-fixture
```

`tmp/seed/` is gitignored, so seed outputs never accidentally land in commits.

## Resolving the `progest` binary

The scripts try the following in order:

1. `$PROGEST_BIN` — point this at any binary you want (e.g. a release build, or a locally-installed `progest`).
2. `target/debug/progest` — used automatically when present.
3. `cargo run --quiet -p progest-cli --` — fallback. Slow on first invocation but needs no setup.

## Patterns

| Script | Purpose | Highlights |
| --- | --- | --- |
| `minimal.sh` | Smoke surface for `progest search` / `tag` / `view` | 9 flat files, 2 tags, no rules |
| `shots-pipeline.sh` | Naming-rule end-to-end test | `chXXX_NNN_<role>_vNN.psd` template + 2 violators |
| `naming-violations.sh` | Cleanup pipeline test | spaces / Pascal / copy suffix / CJK in names |
| `placement-violations.sh` | `[accepts]` + `is:misplaced` test | per-dir `.dirmeta.toml`, schema.toml with custom fields |
| `sequences.sh` | Sequence detection / drift test | canonical 10-member seq + 2 drift candidates + singletons |

Each script ends by printing a "ready" line with a sample command (usually a `progest search` invocation) you can copy-paste to start exploring.

## Conventions

- Output directories are wiped clean on every run. Don't store anything you want to keep there.
- Scripts use `set -euo pipefail` and exit immediately on any sub-command failure.
- All filesystem writes go through helper functions (`seed_touch`, `seed_write`) defined in `_lib.sh`, so the per-pattern scripts stay readable.
- The `lint` step (where applicable) is run with `|| true` because lint can return non-zero by design when violations exist; the seed should still be considered ready.

## Adding a new pattern

1. Add `scripts/seed/<name>.sh`, source `_lib.sh`, and follow the existing layout (header comment + `prepare_dir` / `seed_init` / writes / `seed_scan` / `seed_done`).
2. `chmod +x` it.
3. Add the pattern name to the `PATTERNS` array in `all.sh` so `all.sh` picks it up.
4. Update the table above.
