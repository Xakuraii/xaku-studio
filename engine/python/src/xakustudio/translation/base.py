"""Translation provider interfaces."""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class TranslationItemInput:
    segment_id: str
    segment_index: int
    speaker_name: str
    voice_presentation: str
    start_ms: int
    end_ms: int
    source_text: str


@dataclass(frozen=True)
class TranslatedItemOutput:
    segment_id: str
    translated_text: str
    notes: str = ""


class TranslationProvider(ABC):
    """Abstract interface for replaceable translation engines."""

    provider_id: str
    supported_pairs: set[tuple[str, str]]

    @abstractmethod
    def translate_batch(
        self,
        items: list[TranslationItemInput],
        source_lang: str,
        target_lang: str,
        glossary: list[dict[str, str]],
        scene_context: str,
        previous_context: list[dict[str, str]],
        credentials: dict[str, str],
        options: dict[str, Any],
    ) -> list[TranslatedItemOutput]:
        """Translate a batch of dialogue segments with character and glossary context."""
