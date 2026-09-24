# Third-party notices

## Document Converter

Document Converter uses or downloads these principal components. The locked
graphs in `tools/ocr-sidecar/requirements.lock` and `src-tauri/Cargo.lock` are
authoritative for exact versions.

| Component | Version | License | Distribution role |
|---|---:|---|---|
| PaddleOCR-VL model | 1.6, revision `c5630…b42` | Apache-2.0 | Downloaded on demand; not committed or bundled |
| MLX | 0.32.2 | MIT | Bundled in OCR sidecar |
| MLX-VLM | 0.7.2 | MIT | Bundled in OCR sidecar |
| Transformers | 5.17.0 | Apache-2.0 | Bundled in OCR sidecar |
| pypdfium2 / PDFium | 5.13.0 | BSD-3-Clause / Apache-2.0 and dependency licenses | Bundled PDF renderer |
| Pillow | 12.3.0 | HPND | Bundled image processing |
| PyInstaller | 6.21.0 | GPL-2.0-or-later with bootloader exception | Build/packaging tool and bootloader |
| CPython | 3.10.19 | PSF-2.0 | Private build runtime; embedded in the packaged sidecar |
| python-build-standalone | build 20260211 | MPL-2.0 | Provider of the checksum-pinned developer build runtime |

Source and license references:

- PaddleOCR-VL: https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6
- MLX: https://github.com/ml-explore/mlx
- MLX-VLM: https://github.com/Blaizzy/mlx-vlm
- Transformers: https://github.com/huggingface/transformers
- pypdfium2: https://github.com/pypdfium2-team/pypdfium2
- Pillow: https://github.com/python-pillow/Pillow
- PyInstaller: https://github.com/pyinstaller/pyinstaller
- CPython: https://www.python.org/downloads/release/python-31019/
- python-build-standalone: https://github.com/astral-sh/python-build-standalone

## Second Brain 3D visualization

The Second Brain globe adapts the local renderer, deterministic spherical
layout, orbital scene, and connectivity replay from the AIS-OS `3d-brain`
skill. The source is MIT licensed, copyright © 2026 Nate Herk:
https://github.com/nateherkai/AIS-OS/tree/main/.agents/skills/3d-brain

The runtime visualization uses `3d-force-graph` (MIT) and Three.js (MIT).
Their exact versions are recorded in `pnpm-lock.yaml`.

This inventory is an engineering notice, not legal advice. Before commercial
release, generate a complete SBOM/notices bundle for all transitive Rust and
Python packages and include required full license texts in the `.app`.
