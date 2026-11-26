"""Protocol and lifecycle tests for the persistent Python JSON-RPC worker."""

from __future__ import annotations

import json
import subprocess
import sys
import time
from pathlib import Path

from xakustudio.worker.protocol import PROTOCOL_VERSION


def _spawn_worker() -> subprocess.Popen[str]:
    repo_root = Path(__file__).resolve().parents[3]
    env_python = sys.executable
    return subprocess.Popen(
        [env_python, "-m", "xakustudio.worker"],
        cwd=str(repo_root),
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        bufsize=1,
    )


def _send(proc: subprocess.Popen[str], payload: dict) -> None:
    assert proc.stdin is not None
    proc.stdin.write(json.dumps(payload) + "\n")
    proc.stdin.flush()


def _read_msg(proc: subprocess.Popen[str]) -> dict:
    assert proc.stdout is not None
    line = proc.stdout.readline()
    assert line, "Worker stdout closed unexpectedly"
    return json.loads(line)


def test_handshake_health_and_shutdown() -> None:
    proc = _spawn_worker()
    try:
        _send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": "1",
                "method": "system.handshake",
                "params": {"protocol_version": PROTOCOL_VERSION},
            },
        )
        resp = _read_msg(proc)
        assert resp["id"] == "1"
        assert resp["result"]["protocol_version"] == PROTOCOL_VERSION
        assert resp["result"]["python_version"].startswith("3.12.")
        assert resp["result"]["capabilities"]["cpu_count"] >= 1

        _send(proc, {"jsonrpc": "2.0", "id": "2", "method": "system.health"})
        health = _read_msg(proc)
        assert health["id"] == "2"
        assert health["result"]["status"] == "ok"

        _send(proc, {"jsonrpc": "2.0", "id": "3", "method": "system.shutdown"})
        shut = _read_msg(proc)
        assert shut["result"]["shutting_down"] is True
        assert proc.wait(timeout=5) == 0
    finally:
        if proc.poll() is None:
            proc.kill()


def test_self_test_progress_and_result() -> None:
    proc = _spawn_worker()
    try:
        _send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": "run-1",
                "method": "task.run",
                "params": {
                    "task_id": "t-100",
                    "task_type": "system.self_test",
                    "payload": {"steps": 3, "step_delay_ms": 10},
                },
            },
        )
        messages = []
        while True:
            msg = _read_msg(proc)
            messages.append(msg)
            if msg.get("id") == "run-1":
                break

        progress_msgs = [m for m in messages if m.get("method") == "task.progress"]
        assert len(progress_msgs) == 3
        assert [m["params"]["percent"] for m in progress_msgs] == [33.33, 66.67, 100.0]

        final_msg = messages[-1]
        assert final_msg["result"]["task_id"] == "t-100"
        assert final_msg["result"]["output"]["status"] == "passed"
        assert final_msg["result"]["output"]["steps_completed"] == 3

        _send(proc, {"jsonrpc": "2.0", "id": "bye", "method": "system.shutdown"})
        proc.wait(timeout=5)
    finally:
        if proc.poll() is None:
            proc.kill()


def test_task_cancellation_and_structured_errors() -> None:
    proc = _spawn_worker()
    try:
        # 1. Invalid JSON
        assert proc.stdin is not None
        proc.stdin.write("{broken json\n")
        proc.stdin.flush()
        parse_err = _read_msg(proc)
        assert parse_err["error"]["code"] == -32700

        # 2. Unknown task_type returns ProviderUnavailable (-32002)
        _send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": "unk-1",
                "method": "task.run",
                "params": {"task_id": "t-unk", "task_type": "nonexistent.provider"},
            },
        )
        unk_err = _read_msg(proc)
        assert unk_err["error"]["code"] == -32002
        assert unk_err["error"]["data"]["error_type"] == "ProviderUnavailable"

        # 3. Cancel an active self_test task
        _send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": "long-1",
                "method": "task.run",
                "params": {
                    "task_id": "t-cancel",
                    "task_type": "system.self_test",
                    "payload": {"steps": 20, "step_delay_ms": 100},
                },
            },
        )
        time.sleep(0.05)
        _send(
            proc,
            {
                "jsonrpc": "2.0",
                "id": "cancel-1",
                "method": "task.cancel",
                "params": {"task_id": "t-cancel"},
            },
        )

        responses: dict[str, dict] = {}
        while "long-1" not in responses or "cancel-1" not in responses:
            msg = _read_msg(proc)
            if "id" in msg:
                responses[msg["id"]] = msg

        assert responses["cancel-1"]["result"]["cancelled"] is True
        assert responses["long-1"]["error"]["code"] == -32001
        assert responses["long-1"]["error"]["data"]["error_type"] == "Cancelled"

        _send(proc, {"jsonrpc": "2.0", "id": "bye", "method": "system.shutdown"})
        proc.wait(timeout=5)
    finally:
        if proc.poll() is None:
            proc.kill()
