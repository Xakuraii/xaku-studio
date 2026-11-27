"""Persistent Python worker server handling JSON-RPC 2.0 requests."""

from __future__ import annotations

import json
import os
import platform
import sys
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any, TextIO

from xakustudio.core.capabilities import probe_capabilities
from xakustudio.core.errors import CancelledError, ProviderUnavailableError, WorkerError
from xakustudio.providers import build_default_registry
from xakustudio.worker.protocol import (
    INTERNAL_ERROR,
    INVALID_PARAMS,
    INVALID_REQUEST,
    METHOD_NOT_FOUND,
    PARSE_ERROR,
    PROTOCOL_VERSION,
    JsonRpcTransport,
    StderrLogger,
)


@dataclass
class TaskContext:
    """Execution context passed to worker task handlers."""

    task_id: str
    cancel_event: threading.Event
    emit_progress: Callable[[float, str, str], None]

    def check_cancelled(self) -> None:
        if self.cancel_event.is_set():
            raise CancelledError(self.task_id)


TaskHandler = Callable[[dict[str, Any], TaskContext], dict[str, Any]]


def _handle_self_test(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    """Deterministic diagnostic task to verify progress streaming and cancellation."""
    steps = max(1, min(int(payload.get("steps", 5)), 50))
    step_delay_ms = max(0, min(int(payload.get("step_delay_ms", 50)), 2000))

    for index in range(1, steps + 1):
        ctx.check_cancelled()
        percent = (index / steps) * 100.0
        ctx.emit_progress(percent, "self_test", f"Self-test step {index}/{steps}")
        if step_delay_ms > 0 and ctx.cancel_event.wait(step_delay_ms / 1000.0):
            raise CancelledError(ctx.task_id)

    return {
        "status": "passed",
        "steps_completed": steps,
        "python_version": platform.python_version(),
    }


def _lazy_stt(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    from xakustudio.transcription.whisper_provider import handle_transcribe

    return handle_transcribe(payload, ctx)


def _lazy_diarize(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    from xakustudio.diarization.pyannote_provider import handle_diarize

    return handle_diarize(payload, ctx)


def _lazy_voice_analyze(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    from xakustudio.voices.analysis import handle_voice_analyze

    return handle_voice_analyze(payload, ctx)


def _lazy_translate(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    from xakustudio.translation.gemini_provider import handle_translate

    return handle_translate(payload, ctx)


def _lazy_tts_synthesize(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    from xakustudio.voices.edge_tts_provider import handle_tts_synthesize

    return handle_tts_synthesize(payload, ctx)


def _lazy_tts_list_voices(payload: dict[str, Any], ctx: TaskContext) -> dict[str, Any]:
    from xakustudio.voices.edge_tts_provider import handle_list_voices

    return handle_list_voices(payload, ctx)


class WorkerServer:
    """Reads NDJSON JSON-RPC requests from stdin and dispatches handlers."""

    def __init__(
        self,
        in_stream: TextIO | None = None,
        out_stream: TextIO | None = None,
        err_stream: TextIO | None = None,
    ) -> None:
        self._in = in_stream or sys.stdin
        self._transport = JsonRpcTransport(out_stream)
        self._logger = StderrLogger(err_stream)
        self._registry = build_default_registry()
        self._started_at = time.monotonic()
        self._shutdown_event = threading.Event()
        self._tasks_lock = threading.Lock()
        self._active_tasks: dict[str, threading.Event] = {}
        self._task_threads: list[threading.Thread] = []
        self._loaded_models: set[str] = set()
        self._task_handlers: dict[str, TaskHandler] = {
            "system.self_test": _handle_self_test,
            "stt.transcribe": _lazy_stt,
            "diarization.run": _lazy_diarize,
            "voice.analyze": _lazy_voice_analyze,
            "translation.run": _lazy_translate,
            "tts.synthesize": _lazy_tts_synthesize,
            "tts.list_voices": _lazy_tts_list_voices,
        }

    def register_task_handler(self, task_type: str, handler: TaskHandler) -> None:
        self._task_handlers[task_type] = handler

    def run(self) -> int:
        self._logger.info(
            "worker_started",
            pid=os.getpid(),
            python_version=platform.python_version(),
            protocol_version=PROTOCOL_VERSION,
        )

        while not self._shutdown_event.is_set():
            raw_line = self._in.readline()
            if raw_line == "":
                # Parent closed stdin; shut down cleanly.
                break
            line = raw_line.strip()
            if not line:
                continue
            self._dispatch_line(line)

        self._cancel_all_active_tasks()
        self._join_task_threads(timeout=2.0)
        self._logger.info("worker_stopped", pid=os.getpid())
        return 0

    def _dispatch_line(self, line: str) -> None:
        try:
            msg = json.loads(line)
        except json.JSONDecodeError as exc:
            self._transport.send_error(None, PARSE_ERROR, f"Invalid JSON: {exc.msg}")
            return

        if not isinstance(msg, dict) or msg.get("jsonrpc") != "2.0":
            req_id = msg.get("id") if isinstance(msg, dict) else None
            self._transport.send_error(req_id, INVALID_REQUEST, "Invalid JSON-RPC 2.0 request")
            return

        req_id = msg.get("id")
        method = msg.get("method")
        params = msg.get("params") or {}

        if not isinstance(method, str) or not isinstance(params, dict):
            self._transport.send_error(req_id, INVALID_REQUEST, "Method must be string and params object")
            return

        if method == "system.handshake":
            self._handle_handshake(req_id, params)
        elif method == "system.health":
            self._handle_health(req_id)
        elif method == "system.capabilities":
            self._transport.send_result(
                req_id, probe_capabilities(self._registry.list_metadata())
            )
        elif method == "system.shutdown":
            self._shutdown_event.set()
            self._transport.send_result(req_id, {"shutting_down": True})
        elif method == "task.run":
            self._handle_task_run(req_id, params)
        elif method == "task.cancel":
            self._handle_task_cancel(req_id, params)
        elif method == "model.unload":
            self._handle_model_unload(req_id, params)
        else:
            self._transport.send_error(req_id, METHOD_NOT_FOUND, f"Unknown method '{method}'")

    def _handle_handshake(self, req_id: Any, params: dict[str, Any]) -> None:
        requested = str(params.get("protocol_version", PROTOCOL_VERSION))
        if requested.split(".")[0] != PROTOCOL_VERSION.split(".")[0]:
            self._transport.send_error(
                req_id,
                INVALID_PARAMS,
                f"Incompatible protocol version '{requested}'; worker supports '{PROTOCOL_VERSION}'",
            )
            return

        result = {
            "protocol_version": PROTOCOL_VERSION,
            "python_version": platform.python_version(),
            "pid": os.getpid(),
            "capabilities": probe_capabilities(self._registry.list_metadata()),
        }
        self._transport.send_result(req_id, result)

    def _handle_health(self, req_id: Any) -> None:
        with self._tasks_lock:
            active_ids = list(self._active_tasks.keys())
        self._transport.send_result(
            req_id,
            {
                "status": "ok",
                "pid": os.getpid(),
                "uptime_seconds": round(time.monotonic() - self._started_at, 3),
                "active_tasks": active_ids,
                "loaded_models": sorted(self._loaded_models),
            },
        )

    def _handle_task_run(self, req_id: Any, params: dict[str, Any]) -> None:
        task_id = params.get("task_id")
        task_type = params.get("task_type")
        payload = params.get("payload") or {}

        if not isinstance(task_id, str) or not task_id:
            self._transport.send_error(req_id, INVALID_PARAMS, "'task_id' must be a non-empty string")
            return
        if not isinstance(task_type, str) or not task_type:
            self._transport.send_error(req_id, INVALID_PARAMS, "'task_type' must be a non-empty string")
            return
        if not isinstance(payload, dict):
            self._transport.send_error(req_id, INVALID_PARAMS, "'payload' must be an object")
            return

        handler = self._task_handlers.get(task_type)
        if handler is None:
            err = ProviderUnavailableError(
                task_type, f"No handler registered for task_type '{task_type}'"
            ).to_rpc_error()
            self._transport.send_error(req_id, err["code"], err["message"], err["data"])
            return

        cancel_event = threading.Event()
        with self._tasks_lock:
            if task_id in self._active_tasks:
                self._transport.send_error(
                    req_id, INVALID_PARAMS, f"Task '{task_id}' is already running"
                )
                return
            self._active_tasks[task_id] = cancel_event

        thread = threading.Thread(
            target=self._execute_task_thread,
            args=(req_id, task_id, task_type, payload, handler, cancel_event),
            daemon=True,
            name=f"xaku-task-{task_id[:8]}",
        )
        with self._tasks_lock:
            self._task_threads = [t for t in self._task_threads if t.is_alive()]
            self._task_threads.append(thread)
        thread.start()

    def _execute_task_thread(
        self,
        req_id: Any,
        task_id: str,
        task_type: str,
        payload: dict[str, Any],
        handler: TaskHandler,
        cancel_event: threading.Event,
    ) -> None:
        ctx = TaskContext(
            task_id=task_id,
            cancel_event=cancel_event,
            emit_progress=lambda pct, stage, msg: self._transport.emit_progress(
                task_id, pct, stage, msg
            ),
        )
        try:
            ctx.check_cancelled()
            result = handler(payload, ctx)
            self._transport.send_result(req_id, {"task_id": task_id, "output": result})
        except WorkerError as exc:
            rpc_err = exc.to_rpc_error()
            self._transport.send_error(req_id, rpc_err["code"], rpc_err["message"], rpc_err["data"])
        except Exception as exc:  # noqa: BLE001
            self._logger.error("task_execution_failed", task_id=task_id, task_type=task_type, error=str(exc))
            self._transport.send_error(
                req_id,
                INTERNAL_ERROR,
                f"Internal worker error: {exc}",
                {"error_type": "TaskExecutionError", "retryable": False, "task_id": task_id},
            )
        finally:
            with self._tasks_lock:
                self._active_tasks.pop(task_id, None)

    def _handle_task_cancel(self, req_id: Any, params: dict[str, Any]) -> None:
        task_id = params.get("task_id")
        if not isinstance(task_id, str) or not task_id:
            self._transport.send_error(req_id, INVALID_PARAMS, "'task_id' must be a non-empty string")
            return
        with self._tasks_lock:
            cancel_event = self._active_tasks.get(task_id)
        if cancel_event is None:
            self._transport.send_result(req_id, {"task_id": task_id, "cancelled": False, "found": False})
            return
        cancel_event.set()
        self._transport.send_result(req_id, {"task_id": task_id, "cancelled": True, "found": True})

    def _handle_model_unload(self, req_id: Any, params: dict[str, Any]) -> None:
        model_id = params.get("model_id")
        if isinstance(model_id, str) and model_id:
            self._loaded_models.discard(model_id)
        else:
            self._loaded_models.clear()

        from xakustudio.diarization.pyannote_provider import unload_all as unload_diar
        from xakustudio.transcription.whisper_provider import unload_all as unload_stt

        unload_stt()
        unload_diar()

        cuda_cleared = False
        try:
            import torch  # type: ignore[import-not-found]

            if torch.cuda.is_available():
                torch.cuda.empty_cache()
                cuda_cleared = True
        except ImportError:
            pass

        self._transport.send_result(
            req_id,
            {"unloaded": True, "cuda_cache_cleared": cuda_cleared, "loaded_models": sorted(self._loaded_models)},
        )

    def _cancel_all_active_tasks(self) -> None:
        with self._tasks_lock:
            for event in self._active_tasks.values():
                event.set()

    def _join_task_threads(self, timeout: float) -> None:
        with self._tasks_lock:
            threads = list(self._task_threads)
        deadline = time.monotonic() + timeout
        for thread in threads:
            remaining = max(0.0, deadline - time.monotonic())
            thread.join(timeout=remaining)
