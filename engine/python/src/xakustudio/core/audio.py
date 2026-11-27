"""Lightweight WAV audio loading utilities using Python stdlib + NumPy."""

from __future__ import annotations

import wave
from pathlib import Path

import numpy as np


def load_wav_mono_float32(path: str | Path) -> tuple[np.ndarray, int]:
    """Load a 16-bit PCM WAV file into a mono float32 array normalized to [-1, 1]."""
    wav_path = Path(path)
    if not wav_path.is_file():
        raise FileNotFoundError(f"Audio file not found: {wav_path}")

    with wave.open(str(wav_path), "rb") as wf:
        channels = wf.getnchannels()
        sampwidth = wf.getsampwidth()
        sample_rate = wf.getframerate()
        n_frames = wf.getnframes()
        raw = wf.readframes(n_frames)

    if sampwidth != 2:
        raise ValueError(f"Unsupported WAV sample width {sampwidth * 8}-bit; expected 16-bit PCM")

    samples = np.frombuffer(raw, dtype=np.int16).astype(np.float32) / 32768.0
    if channels > 1:
        samples = samples.reshape(-1, channels).mean(axis=1)
    return samples, sample_rate
