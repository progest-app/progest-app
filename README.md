# Progest

> Metadata-first file management for creative production projects.

Progest is a local-first, OSS tool that brings naming-convention enforcement, sidecar metadata (`.meta` files), and fast full-project search to creative pipelines — film, game, 3DCG, VFX.

**Status:** Pre-alpha, active development. Target v1.0: macOS first, Q3 2026.

[日本語 README](./README.ja.md)

---

## Why Progest

Creative projects drown in files: layer stacks, versions, renders, references, caches. Existing tools either lock you into proprietary asset servers or leave you with ad-hoc folder conventions that degrade the moment anyone new joins.

Progest sits next to your existing directories, learns the rules you already want, and makes them enforceable, searchable, and shareable — without taking your files hostage.

**Design principles**

- **Your filesystem stays the source of truth.** We write sidecar `.meta` files next to yours. Never import, never hide.
- **Rules come first.** Naming conventions are a first-class citizen, not a lint plugin afterthought.
- **CLI is equal to GUI.** Everything the UI does, the CLI does — for pipelines and automation.
- **Git-friendly by default.** `.meta` is plain TOML, designed for merge drivers, diffable, reviewable.
- **No lock-in.** Delete Progest and your files are still just files.

---

## Core features (v1 MVP)

| Feature | Notes |
| --- | --- |
| Naming rule DSL | Template syntax `{prefix}_{seq:03d}_{desc:snake}_v{version:02d}.{ext}`, plus loose constraint rules (charset, casing, forbidden characters) |
| Rule modes | `strict` / `warn` (default) / `hint` / `off`, per-directory with inheritance |
| `.meta` sidecar | TOML, UUID-based, versioned schema, section-isolated to reduce merge conflicts |
| Copy semantics | Copies always get a new UUID and record `source_file_id`. Conflicts surface in UI |
| Search | `tag:character type:psd is:violation` style DSL, shared between GUI command palette and CLI |
| Flat view & saved views | Query-driven browsing, shareable smart folders via `.progest/views.toml` |
| Thumbnails | Image (PNG/JPEG/WebP/TIFF/HEIC), video (via ffmpeg), PSD embedded previews |
| AI naming assist | BYOK (OpenAI / Anthropic-compatible). Keys in OS keychain. Local-only context |
| Templates | Export project structure, rules, schema, and saved views as a single TOML |
| External integration | Drag & drop in and out, open-in-external-app |
| CLI | `progest init/scan/lint/rename/tag/search/doctor/meta-merge` |
| i18n | Japanese + English UI |

---

## Roadmap

| Version | Highlights |
| --- | --- |
| **v1.0** (macOS) | Core MVP features above. Target Q3 2026 |
| **v1.1** | Windows support (long paths, file-lock resilience, OneDrive awareness), git-URL templates, Blender thumbnails, lindera Japanese morphology, OS file manager integration |
| **v1.2** | Extended history/undo, encrypted meta, cross-project references |
| **v2.0** | Lua extension API (sandboxed), optional paid cloud sync, template registry |
| **v2.x** | First-class Linux, local LLM support, DCC integration APIs |

---

## Tech stack

- **Core**: Rust
- **UI shell**: [Tauri](https://tauri.app/) v2
- **UI**: React + [shadcn/ui](https://ui.shadcn.com/)
- **Frontend toolchain**: [Vite+](https://viteplus.dev/) (unified Rolldown / oxlint / oxfmt / tsgo)
- **Index**: SQLite + FTS5
- **Monorepo**:
  - `crates/progest-core` — domain logic
  - `crates/progest-cli` — CLI
  - `crates/progest-merge` — git merge driver for `.meta`
  - `crates/progest-tauri` — Tauri IPC glue
  - `app/` — frontend

See [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) for architecture detail.

---

## Development

Progest uses [mise](https://mise.jdx.dev/) as the single source of truth for toolchains (Rust, Node, pnpm) and common workflow tasks. Installing mise is the only prerequisite for a new clone.

```bash
# One-time: install mise (macOS / Linux)
curl https://mise.run | sh

# From the repo root: mise auto-installs the pinned versions of rust, node, pnpm
mise install

# Install frontend deps (runs on-demand from other tasks too)
mise run install
```

### Everyday commands

| Command | What it does |
| --- | --- |
| `mise run check` | rustfmt `--check`, clippy `-D warnings`, oxfmt + oxlint (via `vp check`), `tsc --noEmit`. **Required to pass before every commit.** |
| `mise run test` | `cargo test --workspace` |
| `mise run build` | `cargo build --workspace` + `vp build` (Rolldown) |
| `mise run fmt` | `cargo fmt --all` + `vp fmt` |
| `mise run dev` | Vite dev server only (frontend iteration, no desktop shell) |
| `mise run tauri-dev` | Full desktop app in dev mode (starts Vite + Tauri window) |
| `mise run tauri-build` | Release desktop bundle |
| `mise run cli -- <args>` | Run the `progest` CLI (e.g. `mise run cli -- scan`) |

Raw commands still work — `cargo test`, `pnpm --filter @progest/app dev`, `pnpm tauri:dev` — but `mise run` matches exactly what CI executes, so local passing means CI passes.

### Project layout

```
.
├── app/                     # Vite+ + React 19 + TS frontend (pnpm workspace member)
├── crates/
│   ├── progest-core/        # all domain logic
│   ├── progest-cli/         # `progest` binary
│   ├── progest-merge/       # git merge driver for .meta
│   └── progest-tauri/       # Tauri v2 desktop shell (+ tauri.conf.json)
├── docs/                    # requirements, implementation plan
└── mise.toml                # pinned toolchains + workflow tasks
```

---

## Documentation

- [docs/REQUIREMENTS.md](./docs/REQUIREMENTS.md) — Full requirements spec (Japanese)
- [docs/IMPLEMENTATION_PLAN.md](./docs/IMPLEMENTATION_PLAN.md) — Implementation plan and milestones (Japanese)
- [docs/_DRAFT.md](./docs/_DRAFT.md) — Original requirements draft (Japanese, historical)
- [CLAUDE.md](./CLAUDE.md) — Working notes for Claude Code contributors

---

## Status

Progest is in active design. The repo currently contains requirements and planning docs only. Code is being written toward M0 (skeleton) at the time of this README.

Feedback, especially from film/game/VFX pipeline practitioners, is extremely welcome — please open a GitHub issue.

---

## License

Progest is licensed under the **Apache License, Version 2.0**. See [LICENSE](./LICENSE).

### Bundled third-party software

Progest ships with [FFmpeg](https://ffmpeg.org/) as a separately-invoked subprocess for video thumbnail generation. The bundled build uses the **LGPL 2.1+** configuration only (no `--enable-gpl`, no `--enable-nonfree`). Full license text, build configuration, and source-code acquisition instructions are provided under `LICENSES/ffmpeg/` and in the in-app About screen, in compliance with the LGPL.

### Contributing

We accept contributions under the [Developer Certificate of Origin](https://developercertificate.org/). Add `Signed-off-by: Your Name <you@example.com>` to every commit (`git commit -s` does this automatically). We do not use a CLA.

### User-authored content

Rule files, schemas, saved views, and templates you create are entirely yours — Progest claims no license over your configuration. When the v2 template registry launches, contributors will declare licenses per-template.

---

## Name

*Progest* = **Pro**ject + Manage (suggest / digest / ingest) — a tool that helps you manage and make sense of project files.
