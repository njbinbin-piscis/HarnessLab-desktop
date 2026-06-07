# HarnessLab Desktop

**A desktop app to visually design, run and ship pluggable agent harnesses and
multi-agent systems — Rust + Tauri, no Python.**

This repository is the desktop edition of [HarnessLab](../HarnessLab). The web
platform's Python engine is replaced by an **in-tree native Rust runtime**
([`harness-rt`](crates/harness-rt)), the design-time logic lives in
[`harnesslab-core`](crates/harnesslab-core), and the React visual-designer
frontend talks to the core over **Tauri IPC (commands/events)** instead of HTTP
REST/SSE.

See [`docs/desktop-rewrite-plan.md`](docs/desktop-rewrite-plan.md) for the
migration history and phased roadmap.

## Architecture

```
frontend/                 # React + TS + Vite UI (@xyflow/react canvas)
  src/api.ts              #   Tauri IPC client (replaces HTTP REST/SSE)
src-tauri/                # Tauri v2 desktop shell
  src/commands.rs         #   #[tauri::command] surface
crates/
  harness-rt/             # Native agent runtime (no external piscis-engine dep)
    llm/                  #   OpenAI-compatible + Claude clients (reqwest)
    agent/                  #   AgentLoop, 8 loop strategies, compaction, hooks
    tools/                  #   file/shell/web_search/url_fetcher + register_algorithm
    memory/                 #   in-memory vector + SQLite backends
    scoring/                #   8 native eval scorers
  harnesslab-core/        # Project model, ops, registry, runner, eval, market,
                          #   skill/algorithm persistence + headless CLI
```

Data flows: **UI → Tauri commands → harnesslab-core → harness-rt → LLM API**.

There is **no** `piscis-kernel` git dependency; the runtime is fully owned in
this workspace.

## Features (v0.2)

| Area | What ships |
|------|------------|
| **Runtime** | Native `harness-rt`: real loop/compaction/memory/context algorithms, no silent fallbacks |
| **Libraries** | Agent, Algorithm, Tool (platform-tagged), Skill (ClawHub search/install) |
| **Projects** | Visual canvas, Edit-Ops, SQLite persistence, multi-agent coordinators |
| **Run console** | Live streaming runs, session tree, per-agent metrics |
| **Evaluation** | Built-in datasets, 8 scorers, experiment dashboard + leaderboard |
| **Meta-agent** | System **Architect** agent with `register_algorithm` hot-register |
| **Factory** | Python / TypeScript / Rust codegen + zip export |
| **Market** | Local package install (signed tarballs, registry protocol v1) |
| **i18n** | English + 中文 UI |

## Install

Pre-built bundles are attached to [GitHub Releases](https://github.com/njbinbin-piscis/agent-workbench/releases):

| Platform | Artifacts |
|----------|-----------|
| Linux x86_64 | `.deb`, `.AppImage` |
| Linux ARM64 | `.deb`, `.AppImage` |
| Windows x86_64 / ARM64 | `.msi`, `.exe` (NSIS) |
| macOS (Apple Silicon) | `.dmg`, `.app` |
| macOS Universal | `.dmg` (Intel + Apple Silicon) |

## Develop

Prerequisites: Rust stable, Node.js 20+, and Tauri system deps on Linux
(`libwebkit2gtk-4.1`, `libgtk-3`, etc.).

```bash
# JS deps (root Tauri CLI + frontend)
npm install
npm install --prefix frontend

# dev: Vite on :1420 + Tauri window
npm run tauri:dev

# release bundle (local)
npm run tauri:build
```

Pure-Rust workflows (no GUI):

```bash
cargo fmt --check
cargo test
cargo build -p harnesslab-core --bin harnesslab-headless --release
cargo build -p harnesslab-desktop --release
```

### LLM credentials

Agent runs resolve provider credentials from the environment (same convention as
the Piscis ecosystem):

```bash
export PISCIS_PROVIDER=openai   # or anthropic, deepseek, qwen, …
export PISCIS_API_KEY=sk-…
export PISCIS_BASE_URL=https://…   # optional, for compatible endpoints
```

Configure per-agent overrides in the project designer (credentials slot:
`env` or `static`).

## Configuration reference

YAML agent configs consumed by `harnesslab-core` and the headless CLI are
documented in `crates/harnesslab-core/src/config.rs`; examples live under
`configs/`.

## License

MIT
