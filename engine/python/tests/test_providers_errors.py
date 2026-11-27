"""Worker protocol tests for provider error handling without model downloads."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path


def _spawn_worker() -> subprocess.Popen[str]:
    repo_root = Path(__file__).resolve().parents[3]
    return subprocess.Popen(
        [sys.executable, "-m", "xakustudio.worker"],
        cwd=str(repo_root),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
        text=True,
        encoding="utf-8",
        bufsize=1,
    )


def _rpc(proc: subprocess.Popen[str], req_id: str, method: str, params: dict) -> dict:
    assert proc.stdin is not None and proc.stdout is not None
    proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": req_id, "method": method, "params": params}) + "\n")
    proc.stdin.flush()
    while True:
        line = proc.stdout.readline()
        assert line, "Worker stdout closed unexpectedly"
        msg = json.loads(line)
        if msg.get("id") == req_id:
            return msg


def test_diarize_missing_credential_and_stt_missing_file(tmp_path: Path) -> None:
    proc = _spawn_worker()
    try:
        cap_resp = _rpc(proc, "cap", "system.capabilities", {})
        provider_ids = {p["provider_id"]: p for p in cap_resp["result"]["providers"]}
        assert "faster-whisper" in provider_ids
        assert "pyannote-diarization" in provider_ids
        assert provider_ids["voice-analysis"]["available"] is True

        diar_err = _rpc(
            proc,
            "diar-1",
            "task.run",
            {
                "task_id": "t-diar-nocred",
                "task_type": "diarization.run",
                "payload": {
                    "audio_path": str(tmp_path / "dummy.wav"),
                    "output_path": str(tmp_path / "diar.json"),
                },
            },
        )
        assert diar_err["error"]["code"] == -32004
        assert diar_err["error"]["data"]["error_type"] == "MissingCredential"

        stt_err = _rpc(
            proc,
            "stt-1",
            "task.run",
            {
                "task_id": "t-stt-nofile",
                "task_type": "stt.transcribe",
                "payload": {
                    "audio_path": str(tmp_path / "missing.wav"),
                    "output_path": str(tmp_path / "stt.json"),
                },
            },
        )
        assert "error" in stt_err
        assert stt_err["error"]["code"] == -32006

        _rpc(proc, "bye", "system.shutdown", {})
        assert proc.wait(timeout=5) == 0
    finally:
        if proc.poll() is None:
            proc.kill()
