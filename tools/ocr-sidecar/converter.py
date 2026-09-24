"""Page-oriented document conversion for the local OCR sidecar."""

from __future__ import annotations

import re
from pathlib import Path
from typing import Callable

from PIL import Image, ImageOps

from engine import EngineError, PaddleOcrVlEngine


ProgressCallback = Callable[[str, int | None, int | None, str], None]
SUPPORTED_EXTENSIONS = {".pdf", ".png", ".jpg", ".jpeg"}
MAX_PAGES = 2_000
MAX_IMAGE_PIXELS = 80_000_000


def _digital_text_is_usable(text: str) -> bool:
    normalized = " ".join(text.split())
    if len(normalized) < 80:
        return False
    printable = sum(character.isprintable() for character in normalized)
    alphanumeric = sum(character.isalnum() for character in normalized)
    replacement = normalized.count("�")
    length = max(len(normalized), 1)
    return printable / length >= 0.96 and alphanumeric / length >= 0.45 and replacement / length < 0.01


def _safe_input(value: object) -> Path:
    if not isinstance(value, str) or not value:
        raise EngineError("INVALID_REQUEST", "inputPath is required")
    path = Path(value).expanduser().resolve(strict=False)
    if path.suffix.lower() not in SUPPORTED_EXTENSIONS or not path.is_file():
        raise EngineError("UNSUPPORTED_FILE", "Input must be a readable PDF, PNG, or JPEG")
    return path


def _safe_working_directory(value: object) -> Path:
    if not isinstance(value, str) or not value:
        raise EngineError("INVALID_REQUEST", "workingDirectory is required")
    path = Path(value).expanduser().resolve(strict=False)
    if not path.is_dir() or path.is_symlink():
        raise EngineError("INVALID_REQUEST", "workingDirectory must be a managed directory")
    return path


def _mode(value: object) -> str:
    mode = value if isinstance(value, str) else "automatic"
    if mode not in {"automatic", "force-ocr", "digital-only"}:
        raise EngineError("INVALID_REQUEST", "Unsupported processing mode")
    return mode


def _save_normalized_image(source: Path, destination: Path) -> int:
    try:
        with Image.open(source) as opened:
            width, height = opened.size
            if width * height > MAX_IMAGE_PIXELS:
                raise EngineError("RESOURCE_LIMIT_EXCEEDED", "Image dimensions exceed the safety limit")
            orientation = int(opened.getexif().get(274, 1))
            rotation = {3: 180, 6: 90, 8: 270}.get(orientation, 0)
            normalized = ImageOps.exif_transpose(opened).convert("RGB")
            normalized.save(destination, "PNG", optimize=True)
            return rotation
    except EngineError:
        raise
    except Exception as error:
        raise EngineError("UNSUPPORTED_FILE", "Image could not be decoded") from error


def convert_document(
    request: dict[str, object],
    engine: PaddleOcrVlEngine,
    progress: ProgressCallback,
) -> dict[str, object]:
    source = _safe_input(request.get("inputPath"))
    working_directory = _safe_working_directory(request.get("workingDirectory"))
    processing_mode = _mode(request.get("processingMode"))
    model_path = request.get("modelPath")
    max_tokens = request.get("maxTokens", 2048)

    if source.suffix.lower() == ".pdf":
        return _convert_pdf(
            source,
            working_directory,
            processing_mode,
            model_path,
            max_tokens,
            engine,
            progress,
        )
    if processing_mode == "digital-only":
        raise EngineError("UNSUPPORTED_FILE", "Digital text only cannot process image files")

    page_path = working_directory / "page-001.png"
    progress("rendering", 1, 1, "Preparing image")
    rotation = _save_normalized_image(source, page_path)
    progress("ocr", 1, 1, "Recognizing document structure")
    text, _ = engine.convert_image(str(page_path), model_path, "OCR:", max_tokens)
    return {
        "totalPages": 1,
        "pagesDigital": 0,
        "pagesOcr": 1,
        "processingMode": "ocr",
        "pages": [
            {
                "pageNumber": 1,
                "sourceMode": "ocr",
                "text": text,
                "confidence": None,
                "rotationCorrection": rotation,
                "renderedAssetPath": str(page_path),
            }
        ],
        "warnings": [],
    }


def _convert_pdf(
    source: Path,
    working_directory: Path,
    processing_mode: str,
    model_path: object,
    max_tokens: object,
    engine: PaddleOcrVlEngine,
    progress: ProgressCallback,
) -> dict[str, object]:
    try:
        import pypdfium2 as pdfium

        document = pdfium.PdfDocument(source)
    except Exception as error:
        message = str(error).lower()
        code = "ENCRYPTED_PDF" if "password" in message or "security" in message else "INVALID_PDF"
        public = "This PDF is password protected" if code == "ENCRYPTED_PDF" else "PDF could not be opened"
        raise EngineError(code, public) from error

    total_pages = len(document)
    if total_pages == 0:
        raise EngineError("INVALID_PDF", "PDF contains no pages")
    if total_pages > MAX_PAGES:
        raise EngineError("RESOURCE_LIMIT_EXCEEDED", f"PDF exceeds the {MAX_PAGES} page safety limit")

    pages: list[dict[str, object]] = []
    warnings: list[str] = []
    digital_count = 0
    ocr_count = 0

    for index in range(total_pages):
        page_number = index + 1
        page = document[index]
        digital_text = ""
        try:
            text_page = page.get_textpage()
            try:
                digital_text = text_page.get_text_range().strip()
            finally:
                text_page.close()
        except Exception:
            warnings.append(f"Page {page_number}: embedded text could not be inspected")

        use_digital = processing_mode != "force-ocr" and _digital_text_is_usable(digital_text)
        if processing_mode == "digital-only":
            use_digital = True

        if use_digital:
            progress("extracting", page_number, total_pages, "Extracting embedded text")
            pages.append(
                {
                    "pageNumber": page_number,
                    "sourceMode": "digital",
                    "text": digital_text,
                    "confidence": None,
                    "rotationCorrection": 0,
                    "renderedAssetPath": None,
                }
            )
            digital_count += 1
            if not digital_text:
                warnings.append(f"Page {page_number}: no usable embedded text in digital-only mode")
            page.close()
            continue

        progress("rendering", page_number, total_pages, f"Rendering page {page_number}")
        rendered_path = working_directory / f"page-{page_number:03d}.png"
        try:
            bitmap = page.render(scale=2.0, rotation=0)
            try:
                rendered = bitmap.to_pil().convert("RGB")
            finally:
                bitmap.close()
            if rendered.width * rendered.height > MAX_IMAGE_PIXELS:
                raise EngineError("RESOURCE_LIMIT_EXCEEDED", "Rendered page exceeds the pixel safety limit")
            rendered.save(rendered_path, "PNG", optimize=True)
        except EngineError:
            raise
        except Exception as error:
            raise EngineError("INVALID_PDF", f"Page {page_number} could not be rendered") from error

        progress("ocr", page_number, total_pages, f"Recognizing page {page_number}")
        text, _ = engine.convert_image(str(rendered_path), model_path, "OCR:", max_tokens)
        pages.append(
            {
                "pageNumber": page_number,
                "sourceMode": "ocr",
                "text": text,
                "confidence": None,
                "rotationCorrection": 0,
                "renderedAssetPath": str(rendered_path),
            }
        )
        ocr_count += 1
        page.close()

    actual_mode = "hybrid" if digital_count and ocr_count else "ocr" if ocr_count else "digital"
    result = {
        "totalPages": total_pages,
        "pagesDigital": digital_count,
        "pagesOcr": ocr_count,
        "processingMode": actual_mode,
        "pages": pages,
        "warnings": warnings,
    }
    document.close()
    return result
