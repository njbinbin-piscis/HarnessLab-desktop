# HarnessLab Desktop

**A desktop app to visually design, run and ship pluggable agent harnesses and
multi-agent systems — Rust + Tauri, no Python.**

This repository (formerly `agent-workbench`) is being rewritten into a desktop
edition of [HarnessLab](../HarnessLab). The web platform's Python engine is
replaced by the Rust [`piscis-kernel`](https://github.com/njbinbin-piscis/piscis-engine)
runtime, the rest of the backend logic moves into a Rust core crate, and the
React visual-designer frontend is reused — talking to the core over **Tauri
IPC (commands/events)** instead of an HTTP REST/SSE API.

See [`docs/desktop-rewrite-plan.md`](docs/desktop-rewrite-plan.md) for the full
migration plan, module mapping and phased roadmap.

## Architecture

```
frontend/                 # React + TS + Vite UI (ported from HarnessLab; @xyflow/react canvas)
  src/tauri.ts            #   Tauri IPC client (replaces the HTTP REST/SSE data layer)
src-tauri/                # Tauri v2 desktop shell
  src/commands.rs         #   #[tauri::command] surface (replaces FastAPI routes)
  src/lib.rs / main.rs    #   app builder + entry point
crates/
  harnesslab-core/        # Rust core: config/loader (config→HarnessConfig), and — over the
                          #   roadmap — project/ops, registry, experiment, factory, design, market.
                          #   Ships a headless CLI bin (`harnesslab-headless`).
```

The agent runtime (`piscis-kernel`) is consumed as a git dependency, pinned to
the same rev as `openpiscis` / `AgentZ` (see the workspace `Cargo.toml`).

## Status

**P0–P6 — done.** Cargo workspace + Tauri v2 shell loading the ported HarnessLab
frontend through P5 (models, Edit-Ops, SQLite, run console with multi-agent
coordination, Factory codegen, Design Copilot). **P6** adds the decentralized
**Market** (local package install with integrity + HMAC/ed25519 signatures,
palette merge), **Registry protocol v1** ([`docs/market-registry-protocol.md`](docs/market-registry-protocol.md) —
HTTPS tarball / git / `registry:` catalog for future cloud markets), **project templates**, canvas **auto-layout** and **undo/redo**,
and Factory **.zip download** via the native save dialog. See
`docs/desktop-rewrite-plan.md`.

## Develop

Prerequisites: a Rust toolchain, Node.js, and the Tauri Linux system deps
(`libwebkit2gtk-4.1`, `libgtk-3`, etc.).

```bash
# install JS deps (root Tauri CLI + frontend)
npm install
npm install --prefix frontend

# run the desktop app in dev (spawns Vite on :1420, then the Tauri window)
npm run tauri:dev

# build a release bundle
npm run tauri:build
```

Pure-Rust workflows (no GUI):

```bash
cargo build --workspace
cargo test -p harnesslab-core

# headless CLI over the config-driven harness builder
cargo run -p harnesslab-core --bin harnesslab-headless -- --help
```

### Local engine hacking

To hack `piscis-kernel` locally, uncomment the `[patch]` block at the bottom of
the workspace `Cargo.toml` and point it at a sibling `../piscis-engine` checkout.

## Configuration reference

The YAML agent-config format (consumed by `harnesslab-core`'s loader and the
headless CLI) is documented inline in `crates/harnesslab-core/src/config.rs`,
with examples under `configs/`.

## License

MIT
