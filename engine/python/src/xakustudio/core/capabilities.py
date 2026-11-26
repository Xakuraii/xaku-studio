"""Hardware and runtime capability probing without loading AI models."""

from __future__ import annotations

import os
import platform
import shutil
import subprocess
import sys
from typing import Any


def _probe_nvidia_smi() -> tuple[str | None, int | None]:
    """Query GPU name and total VRAM (MiB) via nvidia-smi when torch is not installed."""
    exe = shutil.which("nvidia-smi")
    if not exe:
        return None, None
    try:
        proc = subprocess.run(
            [exe, "--query-gpu=name,memory.total", "--format=csv,noheader,nounits"],
            capture_output=True,
            text=True,
            timeout=2.0,
            check=False,
        )
        if proc.returncode != 0:
            return None, None
        first_line = proc.stdout.strip().splitlines()[0] if proc.stdout.strip() else ""
        if "," not in first_line:
            return None, None
        name_part, vram_part = [p.strip() for p in first_line.split(",", maxsplit=1)]
        return name_part, int(float(vram_part))
    except (OSError, subprocess.SubprocessError, ValueError, IndexError):
        return None, None


def probe_capabilities(registered_providers: list[dict[str, Any]] | None = None) -> dict[str, Any]:
    """Return actual Python runtime and hardware capabilities."""
    cuda_available = False
    torch_installed = False
    gpu_name: str | None = None
    vram_mb: int | None = None

    try:
        import torch  # type: ignore[import-not-found]

        torch_installed = True
        cuda_available = bool(torch.cuda.is_available())
        if cuda_available:
            props = torch.cuda.get_device_properties(0)
            gpu_name = props.name
            vram_mb = int(props.total_memory // (1024 * 1024))
    except ImportError:
        pass

    if gpu_name is None:
        gpu_name, vram_mb = _probe_nvidia_smi()

    return {
        "python_version": platform.python_version(),
        "python_executable": sys.executable,
        "platform": platform.system(),
        "machine": platform.machine(),
        "cpu_count": os.cpu_count() or 1,
        "torch_installed": torch_installed,
        "cuda_available": cuda_available,
        "gpu_name": gpu_name,
        "vram_mb": vram_mb,
        "providers": registered_providers or [],
    }
