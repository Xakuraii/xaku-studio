---
description: Run full XakuStudio verification across TypeScript, React, Python 3.12.10, and Rust/Tauri
---

Run all checks in a single PowerShell chain after edits:

```powershell
npm run typecheck; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
npm test; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
npm run build; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
uv run ruff check engine/python; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
uv run pytest -v; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml
```
