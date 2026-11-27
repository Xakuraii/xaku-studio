"""Unit and protocol tests for Gemini translation and Khmer TTS providers."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from unittest.mock import MagicMock, patch

import pytest

from xakustudio.core.errors import (
    MissingCredentialError,
    TaskExecutionError,
    UnsupportedLanguageError,
)
from xakustudio.translation.base import TranslationItemInput
from xakustudio.translation.gemini_provider import (
    GeminiTranslationProvider,
    build_translation_prompts,
)
from xakustudio.voices.edge_tts_provider import (
    EdgeNeuralTTSProvider,
    format_edge_pitch,
    format_edge_rate,
)


def test_translation_prompts_and_error_guards() -> None:
    items = [
        TranslationItemInput(
            segment_id="seg-1",
            segment_index=0,
            speaker_name="Lin",
            voice_presentation="feminine",
            start_ms=0,
            end_ms=1800,
            source_text="别这样对我说话！",
        )
    ]
    sys_prompt, user_prompt = build_translation_prompts(
        items=items,
        source_lang="zh",
        target_lang="km",
        glossary=[{"source_term": "Lin", "target_term": "លីន", "notes": "Lead"}],
        scene_context="Tense argument scene",
        previous_context=[],
    )
    assert "Khmer" in sys_prompt
    assert "លីន" in user_prompt
    assert "feminine" in user_prompt

    provider = GeminiTranslationProvider()
    with pytest.raises(UnsupportedLanguageError):
        provider.translate_batch(items, "ja", "km", [], "", [], {"GEMINI_API_KEY": "k"}, {})

    with pytest.raises(MissingCredentialError):
        provider.translate_batch(items, "zh", "km", [], "", [], {}, {})

    fake_interaction = MagicMock()
    fake_interaction.output_text = json.dumps(
        {
            "translations": [
                {
                    "segment_id": "seg-1",
                    "translated_text": "កុំនិយាយជាមួយខ្ញុំបែបនេះ!",
                    "notes": "Firm emotional tone",
                }
            ]
        },
        ensure_ascii=False,
    )
    fake_client = MagicMock()
    fake_client.interactions.create.return_value = fake_interaction

    with patch("google.genai.Client", return_value=fake_client):
        out = provider.translate_batch(
            items,
            "zh",
            "km",
            [],
            "",
            [],
            {"GEMINI_API_KEY": "test-key"},
            {"model": "gemini-4-hydrogen"},
        )
    assert len(out) == 1
    assert out[0].translated_text == "កុំនិយាយជាមួយខ្ញុំបែបនេះ!"


def test_edge_tts_formatting_and_validation(tmp_path: Path) -> None:
    assert format_edge_rate(1.0) == "+0%"
    assert format_edge_rate(1.25) == "+25%"
    assert format_edge_rate(0.8) == "-20%"
    assert format_edge_pitch(0) == "+0Hz"
    assert format_edge_pitch(-12) == "-12Hz"

    tts = EdgeNeuralTTSProvider()
    voices = tts.list_voices()
    voice_ids = {v.voice_id for v in voices}
    assert "km-KH-PisethNeural" in voice_ids
    assert "km-KH-SreymomNeural" in voice_ids

    with pytest.raises(TaskExecutionError):
        tts.synthesize_to_wav("   ", "km-KH-PisethNeural", str(tmp_path / "out.wav"))

    with pytest.raises(UnsupportedLanguageError):
        tts.synthesize_to_wav("សួស្តី", "unknown-voice", str(tmp_path / "out.wav"))


def test_worker_translation_missing_cred_and_tts_voices(tmp_path: Path) -> None:
    repo_root = Path(__file__).resolve().parents[3]
    proc = subprocess.Popen(
        [sys.executable, "-m", "xakustudio.worker"],
        cwd=str(repo_root),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
        text=True,
        encoding="utf-8",
        bufsize=1,
    )
    try:
        assert proc.stdin is not None and proc.stdout is not None

        def rpc(req_id: str, method: str, params: dict) -> dict:
            proc.stdin.write(
                json.dumps({"jsonrpc": "2.0", "id": req_id, "method": method, "params": params})
                + "\n"
            )
            proc.stdin.flush()
            while True:
                msg = json.loads(proc.stdout.readline())
                if msg.get("id") == req_id:
                    return msg

        voices_resp = rpc(
            "v1",
            "task.run",
            {"task_id": "t-voices", "task_type": "tts.list_voices", "payload": {}},
        )
        assert any(
            v["voice_id"] == "km-KH-SreymomNeural"
            for v in voices_resp["result"]["output"]["voices"]
        )

        tr_err = rpc(
            "tr1",
            "task.run",
            {
                "task_id": "t-tr-nocred",
                "task_type": "translation.run",
                "payload": {
                    "source_lang": "zh",
                    "target_lang": "km",
                    "output_path": str(tmp_path / "tr.json"),
                    "segments": [{"id": "s1", "source_text": "你好"}],
                },
            },
        )
        assert tr_err["error"]["code"] == -32004
        assert tr_err["error"]["data"]["error_type"] == "MissingCredential"

        rpc("bye", "system.shutdown", {})
        assert proc.wait(timeout=5) == 0
    finally:
        if proc.poll() is None:
            proc.kill()
