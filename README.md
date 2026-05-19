# agentic-hub

Manage your agentic skills, agents, rules, hooks, and sub-agent specs in one place — and use them across tools, services, projects, and devices.

A Tauri 2.x desktop app that projects a single shared `~/.agentic/` capability tree into Codex, Claude Code, Cursor, and OpenClaw, with stage-then-apply safety, named suites, and per-project workspace patches.

## Status

Pre-implementation. Documentation-first migration from the VS Code extension at [`e-studio-copilot`](https://github.com/arno/e-studio-copilot). Code lands after [PRODUCT.md](PRODUCT.md) and [ARCHITECTURE.md](ARCHITECTURE.md) are reviewed.

## Start here

- [PRODUCT.md](PRODUCT.md) — what we are building, user stories, scope, milestones
- [ARCHITECTURE.md](ARCHITECTURE.md) — system architecture (Rust core + Tauri shell + React UI)
- [AGENTS.md](AGENTS.md) — short guide for AI coding agents working in this repo
- [docs/README.md](docs/README.md) — full doc index (features, tech.modules, tech.reference, tech.development)

## Companion architecture docs

- [ARCHITECTURE.permissions.md](ARCHITECTURE.permissions.md) — Tauri 2.x capability model, trust boundary, Windows symlink constraint
- [ARCHITECTURE.projection.md](ARCHITECTURE.projection.md) — projection engine (scan → plan → apply → rule sync)
- [ARCHITECTURE.workspace.md](ARCHITECTURE.workspace.md) — workspace patch lifecycle, manifest cycle, safety guards

## Features

- **Capability Manager** — scan, inspect, stage, apply across Codex, Claude Code, Cursor, OpenClaw
- **Suite Presets** — named capability sets in `~/.agentic-suites.json`, one-click full-reset apply
- **Workspace Suite Sync** — hard-copy a suite into a project workspace with a manifest cleanup cycle
- **Demo Scaffold** — bootstrap a starter `~/.agentic/` tree on a fresh machine

## License

See [LICENSE](LICENSE).
