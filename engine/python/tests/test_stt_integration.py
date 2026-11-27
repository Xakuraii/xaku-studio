"""Slow integration test verifying real faster-whisper transcription via the worker."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest


def _synthesize_english_wav(wav_path: Path) -> bool:
    script = (
        "Add-Type -AssemblyName System.Speech; "
        "$s = New-Object System.Speech.Synthesis.SpeechSynthesizer; "
        "$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo("
        "16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, "
        "[System.Speech.AudioFormat.AudioChannel]::Mono); "
        f"$s.SetOutputToWaveFile('{wav_path}', $fmt); "
        "$s.Speak('The quick brown fox jumps over the lazy dog.'); "
        "$s.Dispose()"
    )
    proc = subprocess.run(
        ["powershell", "-NoProfile", "-Command", script],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    return proc.returncode == 0 and wav_path.is_file() and wav_path.stat().st_size > 1000


@pytest.mark.slow
def test_real_faster_whisper_tiny_transcription(tmp_path: Path) -> None:
    wav_path = tmp_path / "sapi_sample.wav"
    if not _synthesize_english_wav(wav_path):
        pytest.skip("Windows SAPI SpeechSynthesizer unavailable")

    repo_root = Path(__file__).resolve().parents[3]
    models_dir = repo_root / ".cache" / "models"
    models_dir.mkdir(parents=True, exist_ok=True)
    out_json = tmp_path / "stt_out.json"

    env = os.environ.copy()
    env["XAKU_MODELS_DIR"] = str(models_dir)
    env["HF_HOME"] = str(models_dir / "huggingface")
    env["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"

    proc = subprocess.Popen(
        [sys.executable, "-m", "xakustudio.worker"],
        cwd=str(repo_root),
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
        text=True,
        encoding="utf-8",
        bufsize=1,
    )
    try:
        assert proc.stdin is not None and proc.stdout is not None
        req = {
            "jsonrpc": "2.0",
            "id": "stt-real",
            "method": "task.run",
            "params": {
                "task_id": "t-stt-real",
                "task_type": "stt.transcribe",
                "payload": {
                    "audio_path": str(wav_path),
                    "output_path": str(out_json),
                    "language": "en",
                    "model_size": "tiny",
                    "device": "auto",
                    "unload_after": True,
                },
            },
        }
        proc.stdin.write(json.dumps(req) + "\n")
        proc.stdin.flush()

        resp = None
        while True:
            line = proc.stdout.readline()
            assert line, "Worker closed stdout before finishing transcription"
            msg = json.loads(line)
            if msg.get("id") == "stt-real":
                resp = msg
                break

        assert resp is not None
        assert "result" in resp, f"Transcription failed: {resp}"
        output = resp["result"]["output"]
        print("REAL_STT_DEVICE:", output["device"], output["compute_type"])

        assert out_json.is_file()
        data = json.loads(out_json.read_text(encoding="utf-8"))
        assert len(data["segments"]) >= 1
        combined = " ".join(s["text"] for s in data["segments"]).lower()
        assert "fox" in combined
        assert any(len(s.get("words") or []) > 0 for s in data["segments"])

        proc.stdin.write(json.dumps({"jsonrpc": "2.0", "id": "bye", "method": "system.shutdown"}) + "\n")
        proc.stdin.flush()
        proc.wait(timeout=10)
    finally:
        if proc.poll() is None:
            proc.kill()
