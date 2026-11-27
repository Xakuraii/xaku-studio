"""Provider metadata contracts and registry."""

from __future__ import annotations

import importlib.util

from xakustudio.providers.base import ProviderMetadata, ProviderRegistry


def build_default_registry() -> ProviderRegistry:
    """Register Stage 4–5 AI providers with live package availability."""
    registry = ProviderRegistry()
    fw_ok = importlib.util.find_spec("faster_whisper") is not None
    pyannote_ok = importlib.util.find_spec("pyannote.audio") is not None
    genai_ok = importlib.util.find_spec("google.genai") is not None
    edge_tts_ok = importlib.util.find_spec("edge_tts") is not None

    registry.register(
        ProviderMetadata(
            provider_id="faster-whisper",
            name="Faster-Whisper (CTranslate2)",
            version="1.2.1",
            capability="stt",
            supported_languages=["zh", "en", "auto"],
            supported_features=["word_timestamps", "silero_vad"],
            required_hardware="GPU (4 GB VRAM int8_float16) or CPU int8",
            is_local=True,
            requires_credential=False,
            license="MIT",
            available=fw_ok,
            unavailable_reason=None if fw_ok else "faster-whisper package not installed",
        )
    )
    registry.register(
        ProviderMetadata(
            provider_id="pyannote-diarization",
            name="pyannote.audio Speaker Diarization",
            version="4.1.1",
            capability="diarization",
            supported_languages=["multilingual"],
            supported_features=["speaker_diarization"],
            required_hardware="GPU (4 GB VRAM sequential) or CPU",
            is_local=True,
            requires_credential=True,
            license="MIT / CC-BY-4.0 gated model",
            available=pyannote_ok,
            unavailable_reason=None if pyannote_ok else "pyannote.audio package not installed",
        )
    )
    registry.register(
        ProviderMetadata(
            provider_id="voice-analysis",
            name="Autocorrelation Pitch & Voice Analysis",
            version="1.0.0",
            capability="voice_analyze",
            supported_languages=["multilingual"],
            supported_features=["pitch_estimation", "presentation_estimate"],
            required_hardware="CPU",
            is_local=True,
            requires_credential=False,
            license="MIT",
            available=True,
        )
    )
    registry.register(
        ProviderMetadata(
            provider_id="gemini-translation",
            name="Gemini Context-Aware Translation",
            version="3.6.1",
            capability="translation",
            supported_languages=["zh->km", "zh->en", "en->km", "km->en"],
            supported_features=["character_context", "glossary", "structured_output"],
            required_hardware="Cloud API",
            is_local=False,
            requires_credential=True,
            license="Google Gemini API Terms",
            available=genai_ok,
            unavailable_reason=None if genai_ok else "google-genai package not installed",
        )
    )
    registry.register(
        ProviderMetadata(
            provider_id="edge-neural-tts",
            name="Neural Khmer TTS (Piseth / Sreymom)",
            version="7.3.1",
            capability="tts",
            supported_languages=["km", "en"],
            supported_features=["rate_control", "pitch_control", "multi_voice"],
            required_hardware="Cloud API",
            is_local=False,
            requires_credential=False,
            license="Microsoft Neural Speech / Edge TTS",
            available=edge_tts_ok,
            unavailable_reason=None if edge_tts_ok else "edge-tts package not installed",
        )
    )
    return registry


__all__ = ["ProviderMetadata", "ProviderRegistry", "build_default_registry"]
