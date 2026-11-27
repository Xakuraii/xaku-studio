## Summary

<!-- Concise description of the stage or fix implemented -->

## Architecture & Security Checklist

- [ ] Responsibilities remain cleanly separated across Rust Core, Python 3.12.10 Engine, FFmpeg Media Engine, and React UI.
- [ ] No fake AI results, hardcoded success payloads, or unverified export claims.
- [ ] No API keys or tokens stored in source code, SQLite, or `.xaku` files (OS Credential Manager only).
- [ ] Source media files are preserved non-destructively.

## Verification

- [ ] `npm run typecheck`
- [ ] `npm test`
- [ ] `npm run build`
- [ ] `uv run ruff check engine/python`
- [ ] `uv run pytest -v`
- [ ] `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml`
- [ ] `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml`
