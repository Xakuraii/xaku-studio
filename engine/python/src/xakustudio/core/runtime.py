"""Windows runtime DLL path configuration for CUDA/CTranslate2 interop."""

from __future__ import annotations

import importlib.util
import os
import sys
from pathlib import Path

_DLL_HANDLES: list[object] = []


def configure_windows_dll_paths() -> None:
    """Register PyTorch's bundled lib directory on Windows without importing torch."""
    if sys.platform != "win32" or not hasattr(os, "add_dll_directory"):
        return
    if _DLL_HANDLES:
        return
    spec = importlib.util.find_spec("torch")
    if spec is None or not spec.origin:
        return
    torch_lib = Path(spec.origin).resolve().parent / "lib"
    if torch_lib.is_dir():
        lib_str = str(torch_lib)
        _DLL_HANDLES.append(os.add_dll_directory(lib_str))
        current_path = os.environ.get("PATH", "")
        if lib_str.lower() not in current_path.lower():
            os.environ["PATH"] = f"{lib_str}{os.pathsep}{current_path}" if current_path else lib_str
