"""NDJSON JSON-RPC 2.0 transport and stderr structured logging."""

from __future__ import annotations

import json
import sys
import threading
import time
from typing import Any, TextIO

PROTOCOL_VERSION = "1.0"

PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603


class StderrLogger:
    """Writes structured JSON log records to stderr so stdout stays pure JSON-RPC."""

    def __init__(self, stream: TextIO | None = None) -> None:
        self._stream = stream or sys.stderr
        self._lock = threading.Lock()

    def log(self, level: str, message: str, **fields: Any) -> None:
        record = {
            "ts": time.time(),
            "level": level,
            "message": message,
            **fields,
        }
        line = json.dumps(record, ensure_ascii=False)
        with self._lock:
            self._stream.write(line + "\n")
            self._stream.flush()

    def info(self, message: str, **fields: Any) -> None:
        self.log("INFO", message, **fields)

    def warning(self, message: str, **fields: Any) -> None:
        self.log("WARN", message, **fields)

    def error(self, message: str, **fields: Any) -> None:
        self.log("ERROR", message, **fields)


class JsonRpcTransport:
    """Thread-safe NDJSON writer for JSON-RPC 2.0 responses and notifications."""

    def __init__(self, out_stream: TextIO | None = None) -> None:
        self._out = out_stream or sys.stdout
        self._write_lock = threading.Lock()

    def _send(self, payload: dict[str, Any]) -> None:
        line = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
        with self._write_lock:
            self._out.write(line + "\n")
            self._out.flush()

    def send_result(self, request_id: Any, result: Any) -> None:
        self._send({"jsonrpc": "2.0", "id": request_id, "result": result})

    def send_error(
        self,
        request_id: Any,
        code: int,
        message: str,
        data: dict[str, Any] | None = None,
    ) -> None:
        err: dict[str, Any] = {"code": code, "message": message}
        if data is not None:
            err["data"] = data
        self._send({"jsonrpc": "2.0", "id": request_id, "error": err})

    def send_notification(self, method: str, params: dict[str, Any]) -> None:
        self._send({"jsonrpc": "2.0", "method": method, "params": params})

    def emit_progress(
        self,
        task_id: str,
        percent: float,
        stage: str,
        message: str,
        eta_seconds: int | None = None,
    ) -> None:
        params: dict[str, Any] = {
            "task_id": task_id,
            "percent": round(max(0.0, min(100.0, percent)), 2),
            "stage": stage,
            "message": message,
        }
        if eta_seconds is not None:
            params["eta_seconds"] = eta_seconds
        self.send_notification("task.progress", params)
