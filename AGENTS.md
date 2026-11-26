# XakuStudio — Engineering & Agent Guidelines

## Architecture Boundaries

Keep responsibilities strictly separated across the four system layers:

1. **Rust Core (`apps/desktop/src-tauri/src/`)**: Desktop lifecycle, Tauri commands, SQLite (SQLx), DAG task scheduler, resource-aware concurrency (1 GPU-heavy slot default for 4 GB VRAM safety), download manager, Python worker supervision, and FFmpeg process execution.
2. **Python AI Engine (`engine/python/src/xakustudio/`)**: Persistent Python `3.12.10` worker communicating over NDJSON JSON-RPC 2.0 (`stdin`/`stdout`), AI inference (STT, diarization, translation, TTS, emotion, audio separation, OCR), and provider adapters.
3. **FFmpeg Media Engine**: Media inspection (`ffprobe`), audio extraction, time-stretching/mixing, subtitle rendering, and video encoding (`ffmpeg`). Never reimplement codecs in Rust or Python.
4. **React Frontend (`apps/desktop/src/`)**: Workstation UI (dark mode default, blue primary accent), project/timeline/subtitle/voice editors, Zustand state, and TanStack Query async state.

## Mandatory Development Rules

- **Python Version**: Use Python `3.12.10` via `uv` (`pyproject.toml` + `uv.lock`). Verify Windows + Python 3.12 compatibility before adopting any AI dependency.
- **No Fake Results**: Never fake AI processing, return hardcoded success payloads, or claim an unverified export succeeded. Unavailable providers must return structured errors.
- **Security**: Never hardcode or store API keys in source code, SQLite, or `.xaku` project files. Use OS-backed credential storage and never build shell command strings from user paths.
- **Non-Destructive Media**: Never overwrite imported source media.
- **Testing & Verification**: Run Rust tests (`cargo test`), Python tests (`uv run pytest`), frontend tests (`npm test`), and build checks before committing each stage.
- **Commits**: Use focused conventional commits (e.g., `feat(worker): add persistent Python JSON-RPC worker`).
