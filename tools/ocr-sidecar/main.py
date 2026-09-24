"""Self-contained JSONL process boundary for the Document Converter spike."""

from __future__ import annotations

import json
import multiprocessing
import os
import platform
import sys
from importlib.metadata import PackageNotFoundError, version

from converter import convert_document
from engine import EngineError, PaddleOcrVlEngine
from protocol import PROTOCOL_VERSION, ProtocolError, error_message, message, parse_request


SIDECAR_VERSION = "0.2.1"
MODEL_ID = "PaddlePaddle/PaddleOCR-VL-1.6"
MODEL_VERSION = "1.6"
MODEL_REVISION = "c5630abae1d940eafe0697512a0325494b02ab42"


def package_version(name: str) -> str:
    try:
        return version(name)
    except PackageNotFoundError:
        return "unknown"


def emit(payload: dict[str, object]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def dispatch(request: dict[str, object], engine: PaddleOcrVlEngine) -> bool:
    request_id = request.get("requestId")
    command = request["command"]

    if command == "health":
        emit(
            message(
                "health",
                request_id,
                status="ok",
                sidecarVersion=SIDECAR_VERSION,
                engine="paddleocr-vl",
                engineVersion=package_version("mlx-vlm"),
                runtimeVersion=package_version("mlx"),
                modelRequired=f"{MODEL_ID}@{MODEL_REVISION}",
                architecture=platform.machine(),
                processId=os.getpid(),
                processGroupId=os.getpgrp(),
            )
        )
        return True

    if command == "capabilities":
        emit(
            message(
                "capabilities",
                request_id,
                ocr=True,
                tables=True,
                formulas=True,
                images=True,
                multipage=True,
                languages=["multilingual"],
                cancellation=True,
                phase="document-converter-v1",
            )
        )
        return True

    if command == "convert":
        job_id = request.get("jobId")

        def report(stage: str, page: int | None, total_pages: int | None, label: str) -> None:
            emit(
                message(
                    "progress",
                    request_id,
                    jobId=job_id,
                    stage=stage,
                    page=page,
                    totalPages=total_pages,
                    label=label,
                    indeterminate=page is None or total_pages is None,
                )
            )

        result = convert_document(request, engine, report)
        emit(message("completed", request_id, jobId=job_id, result=result))
        return True

    if command == "load-model":
        engine.load(request.get("modelPath"))
        emit(message("model-loaded", request_id, status="ok"))
        return True

    if command == "convert-image":
        if not engine.is_loaded:
            emit(message("progress", request_id, stage="loading-model", indeterminate=True))
        text, duration_ms = engine.convert_image(
            request.get("imagePath"),
            request.get("modelPath"),
            request.get("instruction", "OCR:"),
            request.get("maxTokens", 1024),
        )
        emit(message("completed", request_id, text=text, durationMs=duration_ms))
        return True

    if command == "cancel":
        raise EngineError(
            "NOT_IMPLEMENTED",
            "Cancellation is not available in the packaging spike",
        )

    if command == "shutdown":
        emit(message("shutdown", request_id, status="ok"))
        return False

    raise ProtocolError("UNKNOWN_COMMAND", "Unsupported sidecar command", request_id)


def main() -> None:
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        print("OCR sidecar requires macOS on Apple Silicon", file=sys.stderr, flush=True)
        raise SystemExit(2)

    engine = PaddleOcrVlEngine()
    for raw_line in sys.stdin:
        request = None
        try:
            request = parse_request(raw_line)
            if not dispatch(request, engine):
                return
        except ProtocolError as error:
            emit(error_message(error))
        except EngineError as error:
            print(f"OCR operation failed: {error.code}", file=sys.stderr, flush=True)
            emit(
                message(
                    "error",
                    request.get("requestId") if request else None,
                    code=error.code,
                    message=error.public_message,
                )
            )
        except Exception as error:
            print(f"Unexpected sidecar error: {type(error).__name__}", file=sys.stderr, flush=True)
            emit(
                message(
                    "error",
                    request.get("requestId") if request else None,
                    code="SIDECAR_INTERNAL_ERROR",
                    message="Document AI encountered an internal error",
                )
            )


def isolate_process_group() -> None:
    """Put the inference runtime and its workers in a killable private group."""
    try:
        os.setsid()
    except PermissionError:
        # This is safe when a launcher already made the process a group leader.
        os.setpgid(0, 0)


if __name__ == "__main__":
    multiprocessing.freeze_support()
    isolate_process_group()
    main()
