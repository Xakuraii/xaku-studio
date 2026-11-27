"""Unit tests for NumPy autocorrelation pitch and voice-presentation analysis."""

from __future__ import annotations

import numpy as np

from xakustudio.voices.analysis import (
    classify_voice_presentation,
    summarize_speaker_pitch,
)


def _sine_wave(freq_hz: float, duration_s: float = 2.0, sr: int = 16000) -> np.ndarray:
    t = np.linspace(0.0, duration_s, int(sr * duration_s), endpoint=False, dtype=np.float32)
    return 0.5 * np.sin(2.0 * np.pi * freq_hz * t)


def _sawtooth_wave(freq_hz: float, duration_s: float = 2.0, sr: int = 16000) -> np.ndarray:
    t = np.linspace(0.0, duration_s, int(sr * duration_s), endpoint=False, dtype=np.float32)
    phase = (t * freq_hz) % 1.0
    return (0.4 * (2.0 * phase - 1.0)).astype(np.float32)


def test_pitch_estimation_within_five_percent() -> None:
    sr = 16000
    for target_hz in (120.0, 220.0):
        med, iqr, voiced = summarize_speaker_pitch(_sine_wave(target_hz, 2.0, sr), sr)
        assert med is not None
        assert iqr is not None
        assert voiced > 1.5
        assert abs(med - target_hz) / target_hz <= 0.05

    med_saw, _, voiced_saw = summarize_speaker_pitch(_sawtooth_wave(110.0, 2.0, sr), sr)
    assert med_saw is not None
    assert voiced_saw > 1.5
    assert abs(med_saw - 110.0) / 110.0 <= 0.05


def test_voice_presentation_classifier_cases() -> None:
    label_m, conf_m = classify_voice_presentation(115.0, 5.0)
    assert label_m == "masculine"
    assert conf_m >= 0.35

    label_f, conf_f = classify_voice_presentation(225.0, 5.0)
    assert label_f == "feminine"
    assert conf_f >= 0.35

    label_ambig, _ = classify_voice_presentation(165.0, 5.0)
    assert label_ambig == "uncertain"

    label_short, conf_short = classify_voice_presentation(115.0, 1.0)
    assert label_short == "uncertain"
    assert conf_short == 0.0
