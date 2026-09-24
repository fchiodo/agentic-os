"""Direct, network-disabled MLX adapter for PaddleOCR-VL.

Rust owns inspection, jobs, verification and reconstruction. This module is
only the engine adapter at the local process boundary.
"""

from __future__ import annotations

import os
import time
from pathlib import Path
from typing import Any


# Loading a model must never trigger an implicit registry request. The Rust
# Model Manager will install and verify every file before this process starts.
os.environ["HF_HUB_OFFLINE"] = "1"
os.environ["TRANSFORMERS_OFFLINE"] = "1"
os.environ["HF_HUB_DISABLE_TELEMETRY"] = "1"
os.environ["DO_NOT_TRACK"] = "1"


class EngineError(RuntimeError):
    def __init__(self, code: str, public_message: str) -> None:
        super().__init__(public_message)
        self.code = code
        self.public_message = public_message


class PaddleOcrVlEngine:
    id = "paddleocr-vl"

    def __init__(self) -> None:
        self._model: Any = None
        self._processor: Any = None
        self._model_path: Path | None = None

    @property
    def is_loaded(self) -> bool:
        return self._model is not None

    def load(self, model_path: object) -> None:
        path = self._validated_model_path(model_path)
        if self._model is not None and self._model_path == path:
            return

        try:
            from mlx_vlm import load

            self._model, self._processor = load(str(path))
            self._model_path = path
        except Exception as error:
            raise EngineError("MODEL_LOAD_FAILED", "Document AI model could not be loaded") from error

    def convert_image(
        self,
        image_path: object,
        model_path: object,
        instruction: object = "OCR:",
        max_tokens: object = 1024,
    ) -> tuple[str, int]:
        image = self._validated_image_path(image_path)
        prompt_text = self._validated_instruction(instruction)
        token_limit = self._validated_token_limit(max_tokens)
        self.load(model_path)

        try:
            from mlx_vlm import generate
            from mlx_vlm.prompt_utils import apply_chat_template

            prompt = apply_chat_template(
                self._processor,
                self._model.config,
                prompt_text,
                num_images=1,
            )
            started = time.monotonic()
            result = generate(
                model=self._model,
                processor=self._processor,
                prompt=prompt,
                image=[str(image)],
                max_tokens=token_limit,
                temperature=0.0,
                verbose=False,
            )
            return result.text, round((time.monotonic() - started) * 1000)
        except Exception as error:
            raise EngineError("OCR_ENGINE_FAILED", "Local OCR inference failed") from error

    @staticmethod
    def _validated_model_path(value: object) -> Path:
        if not isinstance(value, str) or not value:
            raise EngineError("INVALID_REQUEST", "modelPath is required")
        path = Path(value).expanduser().resolve(strict=False)
        required = ("config.json", "model.safetensors", "tokenizer.json")
        if not path.is_dir() or any(not (path / name).is_file() for name in required):
            raise EngineError("MODEL_NOT_INSTALLED", "Document AI model is not installed or incomplete")
        return path

    @staticmethod
    def _validated_image_path(value: object) -> Path:
        if not isinstance(value, str) or not value:
            raise EngineError("INVALID_REQUEST", "imagePath is required")
        path = Path(value).expanduser().resolve(strict=False)
        if path.suffix.lower() not in {".png", ".jpg", ".jpeg"} or not path.is_file():
            raise EngineError("UNSUPPORTED_FILE", "Input must be a readable PNG or JPEG image")
        return path

    @staticmethod
    def _validated_instruction(value: object) -> str:
        allowed = {"OCR:", "Table Recognition:", "Formula Recognition:"}
        if not isinstance(value, str) or value not in allowed:
            raise EngineError("INVALID_REQUEST", "Unsupported recognition instruction")
        return value

    @staticmethod
    def _validated_token_limit(value: object) -> int:
        if isinstance(value, bool) or not isinstance(value, int) or not 1 <= value <= 8192:
            raise EngineError("INVALID_REQUEST", "maxTokens must be between 1 and 8192")
        return value
