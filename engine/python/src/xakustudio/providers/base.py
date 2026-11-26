"""Replaceable AI provider metadata and registry interfaces."""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from typing import Any


@dataclass(frozen=True)
class ProviderMetadata:
    """Self-describing metadata required by every XakuStudio AI provider."""

    provider_id: str
    name: str
    version: str
    capability: str
    supported_languages: list[str]
    supported_features: list[str]
    required_hardware: str
    is_local: bool
    requires_credential: bool
    license: str
    available: bool
    unavailable_reason: str | None = None
    extra: dict[str, Any] = field(default_factory=dict)

    def to_dict(self) -> dict[str, Any]:
        return asdict(self)


class ProviderRegistry:
    """Holds registered provider metadata without fabricating unavailable providers."""

    def __init__(self) -> None:
        self._providers: dict[str, ProviderMetadata] = {}

    def register(self, metadata: ProviderMetadata) -> None:
        self._providers[metadata.provider_id] = metadata

    def list_metadata(self) -> list[dict[str, Any]]:
        return [p.to_dict() for p in self._providers.values()]

    def get(self, provider_id: str) -> ProviderMetadata | None:
        return self._providers.get(provider_id)
