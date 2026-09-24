"""Opt-in real MLX smoke test. Requires an already installed model."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[3]
BINARY = ROOT / "src-tauri" / "binaries" / "ocr-sidecar-aarch64-apple-darwin"
MODEL = os.environ.get("AGENTIC_OS_OCR_MODEL")

if not MODEL:
    raise SystemExit("AGENTIC_OS_OCR_MODEL must point to the verified local model directory")
if not BINARY.is_file():
    raise SystemExit("OCR sidecar is missing; run pnpm prepare:ocr")

with tempfile.TemporaryDirectory(prefix="agentic-os-ocr-smoke-") as temp_dir:
    image_path = Path(temp_dir) / "fixture.png"
    image = Image.new("RGB", (1000, 500), "white")
    draw = ImageDraw.Draw(image)
    font = ImageFont.truetype("/System/Library/Fonts/Supplemental/Arial.ttf", 44)
    draw.text((50, 70), "Agentic OS local OCR smoke test", fill="black", font=font)
    draw.text((50, 150), "Nessun documento viene caricato online.", fill="black", font=font)
    draw.text((50, 230), "Revenue: EUR 2.4B", fill="black", font=font)
    image.save(image_path)

    requests = [
        {
            "protocolVersion": 1,
            "requestId": "real-smoke",
            "command": "convert-image",
            "modelPath": str(Path(MODEL).resolve()),
            "imagePath": str(image_path),
            "maxTokens": 256,
        },
        {
            "protocolVersion": 1,
            "requestId": "shutdown",
            "command": "shutdown",
        },
    ]
    environment = {
        "PATH": "/usr/bin:/bin",
        "HOME": temp_dir,
        "HF_HUB_OFFLINE": "1",
        "TRANSFORMERS_OFFLINE": "1",
    }
    completed = subprocess.run(
        [str(BINARY)],
        input="\n".join(json.dumps(request) for request in requests) + "\n",
        text=True,
        capture_output=True,
        env=environment,
        check=False,
        timeout=300,
    )
    if completed.returncode != 0:
        raise SystemExit(f"sidecar exited {completed.returncode}: {completed.stderr}")
    messages = [json.loads(line) for line in completed.stdout.splitlines()]
    result = next((message for message in messages if message.get("requestId") == "real-smoke" and message.get("type") == "completed"), None)
    if not result or "Agentic OS" not in result.get("text", ""):
        raise SystemExit(f"OCR output did not contain expected text: {completed.stdout}")
    print(json.dumps({"status": "ok", "durationMs": result["durationMs"], "text": result["text"]}, ensure_ascii=False))
