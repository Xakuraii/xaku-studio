# Python AI Engine Dependencies (`3.12.10`)

XakuStudio pins Python `3.12.10` (`requires-python = ">=3.12,<3.13"`) managed via `uv` (`pyproject.toml` + `uv.lock`).

## Verified Windows Stack (Stage 4)

| Package | Resolved Version | Purpose | License |
|---|---|---|---|
| `torch` | `2.9.1+cu128` | Tensor execution & CUDA 12.8 runtime | BSD-3-Clause |
| `torchaudio` | `2.9.1+cu128` | Audio tensor interop for pyannote | BSD-2-Clause |
| `torchcodec` | `0.9.1` | Required dependency of `pyannote.audio` 4.x | BSD-3-Clause |
| `faster-whisper` | `1.2.1` | Speech-to-text + built-in Silero VAD | MIT |
| `ctranslate2` | `4.9.1` | Quantized Transformer inference (`int8_float16` / `int8`) | MIT |
| `pyannote.audio` | `4.1.1` | Speaker diarization pipeline | MIT |
| `numpy` | `2.5.5` | Waveform & autocorrelation pitch analysis | BSD-3-Clause |

## Why CUDA 12.8 (`pytorch-cu128`)

- `ctranslate2` `4.9.1` links against CUDA 12 / cuDNN 9 runtime libraries.
- Installing `torch==2.9.1` + `torchaudio==2.9.1` from `https://download.pytorch.org/whl/cu128` keeps CUDA 12 DLLs (`cublas64_12.dll`, `cudnn64_9.dll`) inside `<venv>/Lib/site-packages/torch/lib`.
- At worker startup (`xakustudio.core.runtime.configure_windows_dll_paths()`), Windows registers `torch/lib` via `os.add_dll_directory(...)` so `ctranslate2` uses GPU acceleration without requiring a global CUDA Toolkit installation.

## TorchCodec & In-Memory Audio Note

`pyannote.audio` 4.x warns if `FFmpeg` shared DLLs are absent for `torchcodec` file decoding. XakuStudio avoids `torchcodec` file decoding altogether by loading extracted `16 kHz` mono PCM WAV stems with Python's `wave` module and passing `{"waveform": tensor, "sample_rate": 16000}` directly to the pyannote pipeline.

## Model Licensing & Credentials

- **Whisper weights** (OpenAI / Systran CTranslate2 conversions): **MIT** — downloaded locally on first transcription run into `<app_data>/models/whisper`.
- **`pyannote/speaker-diarization-community-1`** (or `3.1`): **CC-BY-4.0** (gated on Hugging Face). Requires accepting the model license terms on Hugging Face and storing `HF_TOKEN` in Windows Credential Manager via **Models & Providers**.
