"""Text-to-speech provider interfaces and voice catalog definitions."""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import asdict, dataclass
from typing import Any


@dataclass(frozen=True)
class VoicePreset:
    """Describes an available voice offered by a TTS provider."""

    provider_id: str
    voice_id: str
    display_name: str
    language: str
    gender_presentation: str
    supports_rate: bool
    supports_pitch: bool
    requires_authorization: bool = False
    notes: str = ""

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class TTSProvider(ABC):
    """Abstract interface for replaceable TTS engines."""

    provider_id: str

    @abstractmethod
    def list_voices(self) -> list[VoicePreset]:
        """Return voices genuinely supported by this provider."""

    @abstractmethod
    def synthesize_to_wav(
        self,
        text: str,
        voice_id: str,
        output_wav_path: str,
        speed_ratio: float = 1.0,
        pitch_hz_delta: int = 0,
    ) -> int:
        """Synthesize text to a WAV file and return its duration in milliseconds."""
