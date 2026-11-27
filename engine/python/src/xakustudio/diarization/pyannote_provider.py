"""pyannote.audio speaker diarization provider."""

from __future__ import annotations

import gc
import json
import threading
from pathlib import Path
from typing import TYPE_CHECKING, Any

from xakustudio.core.audio import load_wav_mono_float32
from xakustudio.core.errors import MissingCredentialError, OutOfMemoryError, TaskExecutionError

if TYPE_CHECKING:
    from xakustudio.worker.server import TaskContext

_LOCK = threading.Lock()
_PIPELINE_CACHE: dict[str, Any] = {}


def unload_all() -> None:
    """Release cached diarization pipelines and clear CUDA cache."""
    with _LOCK:
        _PIPELINE_CACHE.clear()
    gc.collect()
    try:
        import torch

        if torch.cuda.is_available():
            torch.cuda.empty_cache()
    except ImportError:
        pass


def _is_auth_or_gated_error(exc: Exception) -> bool:
    msg = str(exc).lower()
    return any(
        k in msg
        for k in ("401", "403", "gated", "unauthorized", "forbidden", "access", "token")
    )


def handle_diarize(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    """Run pyannote speaker diarization on extracted mono WAV audio."""
    credentials = payload.get("credentials") or {}
    hf_token = str(credentials.get("HF_TOKEN") or "").strip()
    if not hf_token:
        raise MissingCredentialError("pyannote-diarization", "HF_TOKEN")

    audio_path = str(payload.get("audio_path") or "").strip()
    output_path = str(payload.get("output_path") or "").strip()
    if not audio_path or not Path(audio_path).is_file():
        raise TaskExecutionError(f"Audio file not found: '{audio_path}'")
    if not output_path:
        raise TaskExecutionError("'output_path' is required for diarization.run")

    model_id = str(payload.get("model_id") or "pyannote/speaker-diarization-community-1")
    requested_device = str(payload.get("device") or "auto").lower()

    import torch
    from pyannote.audio import Pipeline

    pipeline = None
    try:
        ctx.emit_progress(5.0, "loading_model", f"Loading {model_id}")
        ctx.check_cancelled()

        try:
            pipeline = Pipeline.from_pretrained(model_id, token=hf_token)
        except TypeError:
            pipeline = Pipeline.from_pretrained(model_id, use_auth_token=hf_token)
        except Exception as exc:
            if _is_auth_or_gated_error(exc):
                raise MissingCredentialError(
                    "pyannote-diarization",
                    f"HF_TOKEN (accept terms at https://huggingface.co/{model_id})",
                ) from exc
            raise TaskExecutionError(f"Failed to load diarization pipeline: {exc}") from exc

        if pipeline is None:
            raise MissingCredentialError(
                "pyannote-diarization",
                f"HF_TOKEN (accept terms at https://huggingface.co/{model_id})",
            )

        use_cuda = requested_device != "cpu" and torch.cuda.is_available()
        actual_device = "cuda" if use_cuda else "cpu"
        if use_cuda:
            pipeline.to(torch.device("cuda"))

        samples, sr = load_wav_mono_float32(audio_path)
        audio_input = {
            "waveform": torch.from_numpy(samples).unsqueeze(0),
            "sample_rate": sr,
        }

        def _hook(
            step_name: str,
            _step_artefact: Any,
            file: Any = None,
            total: int | None = None,
            completed: int | None = None,
        ) -> None:
            _ = file
            ctx.check_cancelled()
            if total and completed is not None and total > 0:
                pct = min(95.0, 10.0 + (completed / total) * 85.0)
                ctx.emit_progress(pct, "diarizing", f"{step_name} ({completed}/{total})")

        kwargs: dict[str, Any] = {"hook": _hook}
        for opt in ("num_speakers", "min_speakers", "max_speakers"):
            if payload.get(opt) is not None:
                kwargs[opt] = int(payload[opt])

        ctx.emit_progress(10.0, "diarizing", f"Running speaker diarization ({actual_device})")
        try:
            out = pipeline(audio_input, **kwargs)
        except RuntimeError as exc:
            if "out of memory" in str(exc).lower():
                raise OutOfMemoryError(f"GPU out of memory during diarization: {exc}") from exc
            raise

        ann = (
            getattr(out, "exclusive_speaker_diarization", None)
            or getattr(out, "speaker_diarization", None)
            or out
        )

        turns: list[dict[str, Any]] = []
        seen_speakers: list[str] = []
        for segment, _, label in ann.itertracks(yield_label=True):
            label_str = str(label)
            if label_str not in seen_speakers:
                seen_speakers.append(label_str)
            turns.append(
                {
                    "start": round(float(segment.start), 3),
                    "end": round(float(segment.end), 3),
                    "speaker": label_str,
                }
            )

        doc = {
            "model_id": model_id,
            "device": actual_device,
            "speakers": seen_speakers,
            "turns": turns,
        }
        out_file = Path(output_path)
        out_file.parent.mkdir(parents=True, exist_ok=True)
        out_file.write_text(json.dumps(doc, ensure_ascii=False, indent=2), encoding="utf-8")
        ctx.emit_progress(100.0, "completed", f"Detected {len(seen_speakers)} speakers")

        return {
            "output_path": str(out_file),
            "speaker_count": len(seen_speakers),
            "turn_count": len(turns),
            "device": actual_device,
        }
    finally:
        del pipeline
        unload_all()
