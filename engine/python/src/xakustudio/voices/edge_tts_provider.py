"""Neural Khmer & English TTS provider using edge-tts + FFmpeg WAV normalization."""

from __future__ import annotations

import asyncio
import shutil
import subprocess
import wave
from pathlib import Path
from typing import TYPE_CHECKING, Any

from xakustudio.core.errors import TaskExecutionError, UnsupportedLanguageError
from xakustudio.voices.tts_base import TTSProvider, VoicePreset

if TYPE_CHECKING:
    from xakustudio.worker.server import TaskContext

EDGE_VOICES: list[VoicePreset] = [
    VoicePreset(
        provider_id="edge-neural-tts",
        voice_id="km-KH-PisethNeural",
        display_name="Piseth (Khmer Male Neural)",
        language="km",
        gender_presentation="masculine",
        supports_rate=True,
        supports_pitch=True,
        notes="Microsoft Neural Khmer male voice.",
    ),
    VoicePreset(
        provider_id="edge-neural-tts",
        voice_id="km-KH-SreymomNeural",
        display_name="Sreymom (Khmer Female Neural)",
        language="km",
        gender_presentation="feminine",
        supports_rate=True,
        supports_pitch=True,
        notes="Microsoft Neural Khmer female voice.",
    ),
    VoicePreset(
        provider_id="edge-neural-tts",
        voice_id="en-US-GuyNeural",
        display_name="Guy (English Male Neural)",
        language="en",
        gender_presentation="masculine",
        supports_rate=True,
        supports_pitch=True,
        notes="Microsoft Neural English US male voice.",
    ),
    VoicePreset(
        provider_id="edge-neural-tts",
        voice_id="en-US-JennyNeural",
        display_name="Jenny (English Female Neural)",
        language="en",
        gender_presentation="feminine",
        supports_rate=True,
        supports_pitch=True,
        notes="Microsoft Neural English US female voice.",
    ),
]


def format_edge_rate(speed_ratio: float) -> str:
    """Convert numeric speed multiplier (e.g. 1.15 -> '+15%') into edge-tts rate string."""
    clamped = max(0.5, min(2.0, float(speed_ratio)))
    pct = round((clamped - 1.0) * 100)
    return f"+{pct}%" if pct >= 0 else f"{pct}%"


def format_edge_pitch(pitch_hz_delta: int) -> str:
    """Convert integer Hz delta (e.g. -10 -> '-10Hz') into edge-tts pitch string."""
    clamped = max(-50, min(50, int(pitch_hz_delta)))
    return f"+{clamped}Hz" if clamped >= 0 else f"{clamped}Hz"


def _wav_duration_ms(wav_path: Path) -> int:
    with wave.open(str(wav_path), "rb") as wf:
        frames = wf.getnframes()
        rate = wf.getframerate()
    if rate <= 0:
        return 0
    return round((frames / rate) * 1000.0)


def _convert_mp3_to_wav(mp3_path: Path, wav_path: Path) -> int:
    ffmpeg = shutil.which("ffmpeg")
    if not ffmpeg:
        raise TaskExecutionError("FFmpeg is required to convert synthesized TTS audio to WAV")
    wav_path.parent.mkdir(parents=True, exist_ok=True)
    proc = subprocess.run(
        [
            ffmpeg,
            "-v",
            "error",
            "-y",
            "-i",
            str(mp3_path),
            "-ar",
            "48000",
            "-ac",
            "1",
            "-c:a",
            "pcm_s16le",
            str(wav_path),
        ],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    if proc.returncode != 0:
        raise TaskExecutionError(
            f"FFmpeg TTS audio conversion failed: {proc.stderr.strip()}"
        )
    return _wav_duration_ms(wav_path)


class EdgeNeuralTTSProvider(TTSProvider):
    """Neural TTS adapter powered by `edge-tts`."""

    provider_id = "edge-neural-tts"

    def list_voices(self) -> list[VoicePreset]:
        return list(EDGE_VOICES)

    def synthesize_to_wav(
        self,
        text: str,
        voice_id: str,
        output_wav_path: str,
        speed_ratio: float = 1.0,
        pitch_hz_delta: int = 0,
    ) -> int:
        clean_text = text.strip()
        if not clean_text:
            raise TaskExecutionError("Cannot synthesize empty dialogue text")

        known_ids = {v.voice_id for v in EDGE_VOICES}
        if voice_id not in known_ids:
            raise UnsupportedLanguageError(self.provider_id, voice_id)

        import edge_tts

        out_wav = Path(output_wav_path)
        out_wav.parent.mkdir(parents=True, exist_ok=True)
        temp_mp3 = out_wav.with_suffix(".tmp.mp3")

        rate_str = format_edge_rate(speed_ratio)
        pitch_str = format_edge_pitch(pitch_hz_delta)

        async def _save() -> None:
            communicate = edge_tts.Communicate(
                clean_text,
                voice_id,
                rate=rate_str,
                pitch=pitch_str,
            )
            await communicate.save(str(temp_mp3))

        try:
            asyncio.run(_save())
            if not temp_mp3.is_file() or temp_mp3.stat().st_size == 0:
                raise TaskExecutionError("TTS engine produced empty audio", retryable=True)
            return _convert_mp3_to_wav(temp_mp3, out_wav)
        except TaskExecutionError:
            raise
        except Exception as exc:
            raise TaskExecutionError(
                f"Edge Neural TTS synthesis failed: {exc}",
                retryable=True,
            ) from exc
        finally:
            if temp_mp3.exists():
                temp_mp3.unlink(missing_ok=True)


def handle_list_voices(_payload: dict[str, Any], _ctx: TaskContext) -> dict[str, Any]:
    """Return available TTS voices from registered TTS providers."""
    provider = EdgeNeuralTTSProvider()
    return {"voices": [v.to_dict() for v in provider.list_voices()]}


def handle_tts_synthesize(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    """Synthesize one or more dialogue lines to WAV files."""
    provider = EdgeNeuralTTSProvider()
    items = payload.get("items")

    if items is None:
        text = str(payload.get("text") or "")
        voice_id = str(payload.get("voice_id") or "km-KH-SreymomNeural")
        output_path = str(payload.get("output_path") or "").strip()
        if not output_path:
            raise TaskExecutionError("'output_path' is required for tts.synthesize")
        speed_ratio = float(payload.get("speed_ratio") or 1.0)
        pitch_hz_delta = int(payload.get("pitch_hz_delta") or 0)

        ctx.check_cancelled()
        ctx.emit_progress(15.0, "synthesizing", f"Synthesizing with {voice_id}")
        duration_ms = provider.synthesize_to_wav(
            text=text,
            voice_id=voice_id,
            output_wav_path=output_path,
            speed_ratio=speed_ratio,
            pitch_hz_delta=pitch_hz_delta,
        )
        ctx.emit_progress(100.0, "completed", "Voice synthesis complete")
        return {
            "output_path": output_path,
            "duration_ms": duration_ms,
            "voice_id": voice_id,
            "provider_id": provider.provider_id,
        }

    if not isinstance(items, list) or len(items) == 0:
        raise TaskExecutionError("'items' must be a non-empty list")

    results: list[dict[str, Any]] = []
    total = len(items)
    for idx, item in enumerate(items):
        ctx.check_cancelled()
        seg_id = str(item.get("segment_id") or f"item-{idx}")
        text = str(item.get("text") or "")
        voice_id = str(item.get("voice_id") or "km-KH-SreymomNeural")
        output_path = str(item.get("output_path") or "").strip()
        speed_ratio = float(item.get("speed_ratio") or 1.0)
        pitch_hz_delta = int(item.get("pitch_hz_delta") or 0)

        duration_ms = provider.synthesize_to_wav(
            text=text,
            voice_id=voice_id,
            output_wav_path=output_path,
            speed_ratio=speed_ratio,
            pitch_hz_delta=pitch_hz_delta,
        )
        results.append(
            {
                "segment_id": seg_id,
                "output_path": output_path,
                "duration_ms": duration_ms,
                "voice_id": voice_id,
                "provider_id": provider.provider_id,
            }
        )
        ctx.emit_progress(((idx + 1) / total) * 100.0, "synthesizing", f"Generated {idx + 1}/{total} lines")

    return {"items": results, "provider_id": provider.provider_id}
