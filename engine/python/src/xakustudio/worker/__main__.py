"""CLI entrypoint for the XakuStudio persistent Python worker."""

from __future__ import annotations

import sys

from xakustudio.core.runtime import configure_windows_dll_paths
from xakustudio.worker.server import WorkerServer


def main() -> int:
    configure_windows_dll_paths()
    # Ensure UTF-8 encoding on Windows pipes regardless of system locale.
    if hasattr(sys.stdin, "reconfigure"):
        sys.stdin.reconfigure(encoding="utf-8")
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8")
    return WorkerServer().run()


if __name__ == "__main__":
    raise SystemExit(main())
