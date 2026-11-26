# XakuStudio

**AI-Powered Video Localization & Dubbing Studio** (Windows 11 Desktop, local-first, modular, and hardware-aware).

XakuStudio converts videos from one spoken language into another—prioritizing **Chinese short dramas → natural Khmer dubbing & subtitles**—with full human-in-the-loop review and segment-level regeneration.

## Tech Stack

- **Desktop & Core**: Tauri 2, Rust (`tokio`, `serde`, `tracing`, `sqlx` in upcoming stages)
- **Frontend**: React, TypeScript, Vite, Tailwind CSS, Zustand, TanStack Query
- **AI Engine**: Python `3.12.10` managed with `uv` (persistent JSON-RPC 2.0 stdio worker)
- **Media Engine**: FFmpeg & FFprobe

## Prerequisites (Windows 11)

- Rust & Cargo (`x86_64-pc-windows-msvc`)
- Node.js 22+ & npm
- Python `3.12.10` & `uv`
- FFmpeg & FFprobe on `PATH`
- Microsoft Edge WebView2 Runtime & Visual Studio Build Tools

## Development Setup

```powershell
# 1. Sync Python 3.12.10 virtual environment
uv sync

# 2. Install JS dependencies
npm install

# 3. Run tests
uv run pytest
npm test
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

## Documentation

- [Worker Protocol Specification](docs/worker-protocol.md)
- [Engineering Guidelines](AGENTS.md)
