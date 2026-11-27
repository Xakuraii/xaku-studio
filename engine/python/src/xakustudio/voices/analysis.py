"""NumPy autocorrelation pitch and voice-presentation estimation."""

from __future__ import annotations

import json
from pathlib import Path
from typing import TYPE_CHECKING, Any

import numpy as np

from xakustudio.core.audio import load_wav_mono_float32
from xakustudio.core.errors import TaskExecutionError

if TYPE_CHECKING:
    from xakustudio.worker.server import TaskContext


def estimate_frame_pitches(
    samples: np.ndarray,
    sr: int,
    fmin: float = 60.0,
    fmax: float = 400.0,
) -> np.ndarray:
    """Estimate per-frame fundamental frequency (Hz) via normalized autocorrelation."""
    if sr <= 0 or len(samples) == 0:
        return np.array([], dtype=np.float32)

    frame_len = max(64, round(0.04 * sr))
    hop_len = max(16, round(0.01 * sr))
    if len(samples) < frame_len:
        return np.array([], dtype=np.float32)

    min_lag = max(1, int(sr / fmax))
    max_lag = min(frame_len - 2, int(sr / fmin))
    if max_lag <= min_lag:
        return np.array([], dtype=np.float32)

    window = np.hanning(frame_len).astype(np.float32)
    pitches: list[float] = []

    for start in range(0, len(samples) - frame_len + 1, hop_len):
        frame = samples[start : start + frame_len]
        rms = float(np.sqrt(np.mean(frame * frame)))
        if rms < 0.005:
            pitches.append(float("nan"))
            continue

        centered = (frame - float(np.mean(frame))) * window
        energy = float(np.dot(centered, centered))
        if energy <= 1e-9:
            pitches.append(float("nan"))
            continue

        autocorr = np.correlate(centered, centered, mode="full")[frame_len - 1 :] / energy
        search = autocorr[min_lag : max_lag + 1]
        best_rel = int(np.argmax(search))
        best_val = float(search[best_rel])

        if best_val >= 0.5:
            lag = min_lag + best_rel
            pitches.append(float(sr) / float(lag))
        else:
            pitches.append(float("nan"))

    return np.array(pitches, dtype=np.float32)


def summarize_speaker_pitch(
    samples: np.ndarray,
    sr: int,
    hop_seconds: float = 0.01,
) -> tuple[float | None, float | None, float]:
    """Return (median_f0_hz, f0_iqr_hz, voiced_seconds)."""
    pitches = estimate_frame_pitches(samples, sr)
    valid = pitches[~np.isnan(pitches)]
    voiced_seconds = float(len(valid)) * hop_seconds
    if len(valid) == 0:
        return None, None, 0.0

    q25, median, q75 = np.percentile(valid, [25.0, 50.0, 75.0])
    return round(float(median), 2), round(float(q75 - q25), 2), round(voiced_seconds, 3)


def classify_voice_presentation(
    median_f0: float | None,
    voiced_seconds: float,
) -> tuple[str, float]:
    """Estimate masculine/feminine/uncertain voice presentation with confidence."""
    if median_f0 is None or voiced_seconds < 1.5:
        return "uncertain", 0.0

    confidence = min(1.0, abs(median_f0 - 167.5) / 60.0) * min(1.0, voiced_seconds / 5.0)
    confidence = round(confidence, 3)
    if confidence < 0.35:
        return "uncertain", confidence
    if median_f0 <= 150.0:
        return "masculine", confidence
    if median_f0 >= 185.0:
        return "feminine", confidence
    return "uncertain", confidence


def handle_voice_analyze(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    """Analyze speaker pitch and estimated vocal presentation from diarization turns."""
    audio_path = str(payload.get("audio_path") or "").strip()
    diarization_path = str(payload.get("diarization_path") or "").strip()
    output_path = str(payload.get("output_path") or "").strip()

    if not audio_path or not Path(audio_path).is_file():
        raise TaskExecutionError(f"Audio file not found: '{audio_path}'")
    if not diarization_path or not Path(diarization_path).is_file():
        raise TaskExecutionError(f"Diarization file not found: '{diarization_path}'")
    if not output_path:
        raise TaskExecutionError("'output_path' is required for voice.analyze")

    diar = json.loads(Path(diarization_path).read_text(encoding="utf-8"))
    turns = diar.get("turns") or []
    samples, sr = load_wav_mono_float32(audio_path)

    speaker_slices: dict[str, list[np.ndarray]] = {}
    speaker_samples_count: dict[str, int] = {}
    max_samples = 60 * sr

    for turn in turns:
        ctx.check_cancelled()
        label = str(turn.get("speaker") or "")
        if not label:
            continue
        cur_count = speaker_samples_count.get(label, 0)
        if cur_count >= max_samples:
            continue

        s_idx = max(0, round(float(turn["start"]) * sr))
        e_idx = min(len(samples), round(float(turn["end"]) * sr))
        if e_idx <= s_idx:
            continue

        take = min(e_idx - s_idx, max_samples - cur_count)
        speaker_slices.setdefault(label, []).append(samples[s_idx : s_idx + take])
        speaker_samples_count[label] = cur_count + take

    speakers_out: dict[str, dict[str, Any]] = {}
    total_speakers = max(1, len(diar.get("speakers") or speaker_slices))

    for idx, label in enumerate(diar.get("speakers") or list(speaker_slices.keys())):
        ctx.check_cancelled()
        chunks = speaker_slices.get(label, [])
        if chunks:
            joined = np.concatenate(chunks)
            median_f0, iqr_f0, voiced_sec = summarize_speaker_pitch(joined, sr)
        else:
            median_f0, iqr_f0, voiced_sec = None, None, 0.0

        label_est, conf = classify_voice_presentation(median_f0, voiced_sec)
        speakers_out[label] = {
            "median_f0_hz": median_f0,
            "f0_iqr_hz": iqr_f0,
            "voiced_seconds": voiced_sec,
            "voice_presentation": label_est,
            "confidence": conf,
        }
        ctx.emit_progress(((idx + 1) / total_speakers) * 100.0, "analyzing_voice", f"Analyzed {label}")

    doc = {
        "speakers": speakers_out,
        "method": "autocorrelation-v1",
        "note": "Estimated voice presentation, not gender identity",
    }
    out_file = Path(output_path)
    out_file.parent.mkdir(parents=True, exist_ok=True)
    out_file.write_text(json.dumps(doc, ensure_ascii=False, indent=2), encoding="utf-8")

    return {"output_path": str(out_file), "speaker_count": len(speakers_out)}
