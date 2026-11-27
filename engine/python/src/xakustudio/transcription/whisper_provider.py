"""Faster-Whisper STT provider with built-in Silero VAD and cooperative cancellation."""

from __future__ import annotations

import gc
import json
import os
import threading
from pathlib import Path
from typing import TYPE_CHECKING, Any

import numpy as np

from xakustudio.core.audio import load_wav_mono_float32
from xakustudio.core.errors import OutOfMemoryError, ProviderUnavailableError, TaskExecutionError
from xakustudio.core.runtime import configure_windows_dll_paths

if TYPE_CHECKING:
    from xakustudio.worker.server import TaskContext

_CACHE_LOCK = threading.Lock()
_MODEL_CACHE: dict[tuple[str, str, str], Any] = {}


def unload_all() -> None:
    """Release cached Whisper models and clear CUDA cache if loaded."""
    with _CACHE_LOCK:
        _MODEL_CACHE.clear()
    gc.collect()
    try:
        import torch

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except ImportError:
        pass


def _resolve_device_and_compute(
    requested_device: str, requested_compute: str | None
) -> tuple[str, str]:
    import ctranslate2

    if requested_device == "cpu":
        return "cpu", requested_compute or "int8"

    cuda_count = 0
    try:
        cuda_count = int(ctranslate2.get_cuda_device_count())
    except Exception:  # noqa: BLE001
        cuda_count = 0

    if requested_device == "cuda":
        if cuda_count <= 0:
            raise ProviderUnavailableError(
                "faster-whisper", "CUDA requested but CTranslate2 detected 0 CUDA devices"
            )
        return "cuda", requested_compute or "int8_float16"

    # auto mode
    if cuda_count > 0:
        return "cuda", requested_compute or "int8_float16"
    return "cpu", requested_compute or "int8"


def _get_or_load_model(
    model_size: str, device: str, compute_type: str, download_root: str | None
) -> Any:
    from faster_whisper import WhisperModel

    key = (model_size, device, compute_type)
    with _CACHE_LOCK:
        model = _MODEL_CACHE.get(key)
        if model is None:
            model = WhisperModel(
                model_size,
                device=device,
                compute_type=compute_type,
                download_root=download_root,
            )
            _MODEL_CACHE[key] = model
        return model


def _is_cuda_oom_or_runtime_err(exc: Exception) -> bool:
    msg = str(exc).lower()
    return any(
        needle in msg
        for needle in ("out of memory", "cuda", "cublas", "cudnn", "alloc", "driver")
    )


def handle_transcribe(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    """Run faster-whisper transcription with Silero VAD and word-level timestamps."""
    audio_path = str(payload.get("audio_path") or "").strip()
    output_path = str(payload.get("output_path") or "").strip()
    if not audio_path or not Path(audio_path).is_file():
        raise TaskExecutionError(f"Audio file not found: '{audio_path}'")
    if not output_path:
        raise TaskExecutionError("'output_path' is required for stt.transcribe")

    configure_windows_dll_paths()

    language_raw = payload.get("language")
    language = str(language_raw).strip() if language_raw else None
    if language == "auto":
        language = None

    model_size = str(payload.get("model_size") or "medium")
    requested_device = str(payload.get("device") or "auto").lower()
    requested_compute = payload.get("compute_type")
    beam_size = int(payload.get("beam_size") or 5)
    unload_after = bool(payload.get("unload_after", True))
    initial_prompt = payload.get("initial_prompt")
    if not initial_prompt and language == "zh":
        initial_prompt = "以下是普通话的句子。"

    models_dir = os.environ.get("XAKU_MODELS_DIR")
    download_root = str(Path(models_dir) / "whisper") if models_dir else None
    if download_root:
        Path(download_root).mkdir(parents=True, exist_ok=True)

    device, compute_type = _resolve_device_and_compute(
        requested_device, str(requested_compute) if requested_compute else None
    )

    try:
        ctx.emit_progress(2.0, "loading_model", f"Loading Whisper {model_size} ({device})")
        ctx.check_cancelled()

        audio_input: Any = audio_path
        if audio_path.lower().endswith(".wav"):
            samples, sr = load_wav_mono_float32(audio_path)
            if sr != 16000 and len(samples) > 0:
                target_len = max(1, round(len(samples) * 16000 / sr))
                samples = np.interp(
                    np.linspace(0.0, len(samples) - 1, target_len, dtype=np.float32),
                    np.arange(len(samples), dtype=np.float32),
                    samples,
                ).astype(np.float32)
            audio_input = samples

        def _run_once(run_device: str, run_compute: str) -> tuple[dict[str, Any], int]:
            model = _get_or_load_model(model_size, run_device, run_compute, download_root)
            segments_gen, info = model.transcribe(
                audio_input,
                language=language,
                beam_size=beam_size,
                vad_filter=True,
                word_timestamps=True,
                initial_prompt=initial_prompt,
            )
            duration = float(getattr(info, "duration", 0.0) or 0.0)
            segments_out: list[dict[str, Any]] = []

            for idx, seg in enumerate(segments_gen):
                ctx.check_cancelled()
                words_out: list[dict[str, Any]] = []
                if getattr(seg, "words", None):
                    for w in seg.words:
                        words_out.append(
                            {
                                "start": round(float(w.start), 3),
                                "end": round(float(w.end), 3),
                                "word": str(w.word),
                                "probability": round(float(getattr(w, "probability", 0.0) or 0.0), 4),
                            }
                        )
                segments_out.append(
                    {
                        "index": idx,
                        "start": round(float(seg.start), 3),
                        "end": round(float(seg.end), 3),
                        "text": str(seg.text).strip(),
                        "avg_logprob": round(float(getattr(seg, "avg_logprob", 0.0) or 0.0), 4),
                        "no_speech_prob": round(float(getattr(seg, "no_speech_prob", 0.0) or 0.0), 4),
                        "words": words_out,
                    }
                )
                if duration > 0:
                    pct = min(99.0, (float(seg.end) / duration) * 100.0)
                    ctx.emit_progress(pct, "transcribing", f"{seg.end:.1f}s / {duration:.1f}s")

            doc = {
                "language": getattr(info, "language", language) or "unknown",
                "language_probability": round(
                    float(getattr(info, "language_probability", 0.0) or 0.0), 4
                ),
                "duration": round(duration, 3),
                "model": model_size,
                "device": run_device,
                "compute_type": run_compute,
                "segments": segments_out,
            }
            return doc, len(segments_out)

        try:
            doc, seg_count = _run_once(device, compute_type)
        except Exception as exc:
            if requested_device == "cuda" and _is_cuda_oom_or_runtime_err(exc):
                raise OutOfMemoryError(f"CUDA error during transcription: {exc}") from exc
            if requested_device == "auto" and device == "cuda" and _is_cuda_oom_or_runtime_err(exc):
                unload_all()
                device, compute_type = "cpu", "int8"
                ctx.emit_progress(5.0, "fallback_cpu", "CUDA unavailable/OOM; retrying on CPU int8")
                doc, seg_count = _run_once(device, compute_type)
            else:
                raise

        out_file = Path(output_path)
        out_file.parent.mkdir(parents=True, exist_ok=True)
        out_file.write_text(json.dumps(doc, ensure_ascii=False, indent=2), encoding="utf-8")
        ctx.emit_progress(100.0, "completed", f"Transcribed {seg_count} segments")

        return {
            "output_path": str(out_file),
            "segment_count": seg_count,
            "language": doc["language"],
            "device": device,
            "compute_type": compute_type,
        }
    finally:
        if unload_after:
            unload_all()
