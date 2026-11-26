# Rust–Python Worker Communication Protocol (`v1.0`)

XakuStudio runs a **persistent Python `3.12.10` worker** supervised by the Rust core.

## 1. Transport & Framing

- **Transport**: Standard Input (`stdin`) and Standard Output (`stdout`) of the child worker process.
- **Framing**: Newline-delimited JSON (**NDJSON**), one compact UTF-8 JSON object per line (`\n`).
- **Logging Isolation**: `stdout` is strictly reserved for JSON-RPC 2.0 frames. All worker logs are written to `stderr` as JSON lines and forwarded by Rust into `tracing`.
- **Windows Supervision**: Spawned with `CREATE_NO_WINDOW` and bound to a Windows Job Object (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) so the worker terminates cleanly if the desktop app exits or crashes.

## 2. Message Types

### Request (Rust → Python)

```json
{
  "jsonrpc": "2.0",
  "id": "req-1",
  "method": "system.handshake",
  "params": {
    "protocol_version": "1.0",
    "app_version": "0.1.0"
  }
}
```

### Success Response (Python → Rust)

```json
{
  "jsonrpc": "2.0",
  "id": "req-1",
  "result": {
    "protocol_version": "1.0",
    "python_version": "3.12.10",
    "pid": 12345,
    "capabilities": {
      "cuda_available": true,
      "gpu_name": "NVIDIA GeForce GTX 1650",
      "vram_mb": 4096,
      "cpu_count": 12,
      "providers": []
    }
  }
}
```

### Error Response (Python → Rust)

```json
{
  "jsonrpc": "2.0",
  "id": "req-2",
  "error": {
    "code": -32001,
    "message": "Task was cancelled",
    "data": {
      "error_type": "Cancelled",
      "retryable": false,
      "task_id": "task-123"
    }
  }
}
```

### Notification (Python → Rust, no `id`)

```json
{
  "jsonrpc": "2.0",
  "method": "task.progress",
  "params": {
    "task_id": "task-123",
    "percent": 45.0,
    "stage": "running",
    "message": "Processing step 2 of 4"
  }
}
```

## 3. Standard Methods (`v1.0`)

| Method | Direction | Purpose |
|---|---|---|
| `system.handshake` | Rust → Python | Verify protocol version compatibility and probe runtime capabilities. |
| `system.health` | Rust → Python | Periodic heartbeat returning worker uptime, memory usage, and active tasks. |
| `system.capabilities` | Rust → Python | Refresh hardware & installed provider availability. |
| `system.shutdown` | Rust → Python | Request graceful worker exit. |
| `task.run` | Rust → Python | Execute a registered worker task (`task_id`, `task_type`, `payload`). |
| `task.cancel` | Rust → Python | Signal cooperative cancellation for an active `task_id`. |
| `model.unload` | Rust → Python | Explicitly release loaded AI models and clear GPU cache. |
| `task.progress` | Python → Rust | Emit incremental progress updates for a running task. |

## 4. Custom Error Codes

- `-32001`: `Cancelled` — Task cancelled cooperatively via `task.cancel`.
- `-32002`: `ProviderUnavailable` — Requested provider or dependency is not installed/configured.
- `-32003`: `UnsupportedLanguage` — Provider does not support the requested language pair.
- `-32004`: `MissingCredential` — Required API token or credential is missing.
- `-32005`: `OutOfMemory` — GPU VRAM or system RAM exhausted.
- `-32006`: `TaskExecutionError` — Unhandled provider or processing error.
