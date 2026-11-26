"""Structured worker errors mapped to JSON-RPC 2.0 error codes."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class WorkerError(Exception):
    """Base error returned across the Rust-Python JSON-RPC boundary."""

    code: int
    error_type: str
    message: str
    retryable: bool = False
    details: dict[str, Any] | None = None

    def to_rpc_error(self) -> dict[str, Any]:
        data: dict[str, Any] = {
            "error_type": self.error_type,
            "retryable": self.retryable,
        }
        if self.details:
            data.update(self.details)
        return {
            "code": self.code,
            "message": self.message,
            "data": data,
        }


class CancelledError(WorkerError):
    def __init__(self, task_id: str, message: str = "Task was cancelled") -> None:
        super().__init__(
            code=-32001,
            error_type="Cancelled",
            message=message,
            retryable=False,
            details={"task_id": task_id},
        )


class ProviderUnavailableError(WorkerError):
    def __init__(self, provider_id: str, message: str) -> None:
        super().__init__(
            code=-32002,
            error_type="ProviderUnavailable",
            message=message,
            retryable=False,
            details={"provider_id": provider_id},
        )


class UnsupportedLanguageError(WorkerError):
    def __init__(self, provider_id: str, source_lang: str, target_lang: str | None = None) -> None:
        pair = f"{source_lang}->{target_lang}" if target_lang else source_lang
        super().__init__(
            code=-32003,
            error_type="UnsupportedLanguage",
            message=f"Provider '{provider_id}' does not support language '{pair}'",
            retryable=False,
            details={"provider_id": provider_id, "language": pair},
        )


class MissingCredentialError(WorkerError):
    def __init__(self, provider_id: str, credential_name: str) -> None:
        super().__init__(
            code=-32004,
            error_type="MissingCredential",
            message=f"Missing required credential '{credential_name}' for '{provider_id}'",
            retryable=False,
            details={"provider_id": provider_id, "credential": credential_name},
        )


class OutOfMemoryError(WorkerError):
    def __init__(self, message: str = "Insufficient GPU VRAM or system memory") -> None:
        super().__init__(
            code=-32005,
            error_type="OutOfMemory",
            message=message,
            retryable=True,
        )


class TaskExecutionError(WorkerError):
    def __init__(self, message: str, retryable: bool = False, details: dict[str, Any] | None = None) -> None:
        super().__init__(
            code=-32006,
            error_type="TaskExecutionError",
            message=message,
            retryable=retryable,
            details=details,
        )
