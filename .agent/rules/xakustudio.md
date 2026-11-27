# XakuStudio Local Agent Rules

- **Primary Branch**: Work incrementally on `develop`; keep commits focused using Conventional Commits (`type(scope): description`) with `Co-authored-by: Xakuraii <hxungdxakuraii@gmail.com>` when applicable.
- **Python Runtime**: Always use Python `3.12.10` via `uv`. Keep CUDA 12.8 (`cu128`) wheels aligned with `ctranslate2` on Windows.
- **PowerShell Syntax**: Use `; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }` instead of `&&` on Windows PowerShell 5.1.
- **Credentials**: Store `HF_TOKEN`, `GEMINI_API_KEY`, and `AZURE_SPEECH_KEY` only in Windows Credential Manager via `CredentialStore`; inject at execution time without persisting to SQLite.
- **Provider Honesty**: Never simulate translation, transcription, diarization, or TTS output. Report unavailable providers or missing credentials with structured error codes (`-32002` / `-32004`).
