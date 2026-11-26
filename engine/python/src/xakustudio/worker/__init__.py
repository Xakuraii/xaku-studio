"""Persistent JSON-RPC 2.0 stdio worker package."""

from xakustudio.worker.protocol import PROTOCOL_VERSION
from xakustudio.worker.server import WorkerServer

__all__ = ["PROTOCOL_VERSION", "WorkerServer"]
