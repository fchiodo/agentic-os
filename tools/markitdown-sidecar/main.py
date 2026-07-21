"""Narrow, local-only MarkItDown PDF adapter for the Tauri sidecar.

The process accepts exactly one JSON line on stdin containing a base64 PDF and
returns one JSON object on stdout. It never accepts paths or URLs, keeping the
sidecar unable to read arbitrary local files or make conversion-time requests.
"""

from __future__ import annotations

import base64
import binascii
import io
import json
import sys

import markitdown
from markitdown import StreamInfo
from markitdown.converters import PdfConverter


MAX_PDF_BYTES = 2 * 1024 * 1024
MAX_REQUEST_BYTES = (MAX_PDF_BYTES * 4 // 3) + 16 * 1024
MAX_MARKDOWN_BYTES = 8 * 1024 * 1024


def fail(message: str) -> "NoReturn":
    print(message, file=sys.stderr)
    raise SystemExit(2)


def main() -> None:
    raw_request = sys.stdin.buffer.readline(MAX_REQUEST_BYTES + 1)
    if not raw_request:
        fail("missing conversion request")
    if len(raw_request) > MAX_REQUEST_BYTES:
        fail("conversion request exceeds the safety limit")

    try:
        request = json.loads(raw_request)
    except (UnicodeDecodeError, json.JSONDecodeError):
        fail("conversion request is not valid JSON")
    if not isinstance(request, dict) or set(request) != {"pdfBase64"}:
        fail("conversion request must contain only pdfBase64")
    encoded = request.get("pdfBase64")
    if not isinstance(encoded, str):
        fail("pdfBase64 must be a string")

    try:
        pdf = base64.b64decode(encoded, validate=True)
    except (binascii.Error, ValueError):
        fail("pdfBase64 is invalid")
    if len(pdf) > MAX_PDF_BYTES:
        fail("PDF exceeds the 2 MiB safety limit")
    if not pdf.startswith(b"%PDF-"):
        fail("input has no valid PDF signature")

    # The dedicated converter avoids MarkItDown's permissive URI/file router
    # and its format-detection model. This process accepts only verified PDF
    # bytes, so the narrow converter is both safer and faster.
    converter = PdfConverter()
    result = converter.convert(
        io.BytesIO(pdf),
        StreamInfo(
            mimetype="application/pdf",
            extension=".pdf",
            filename="document.pdf",
        ),
    )
    markdown = result.markdown
    if not isinstance(markdown, str):
        fail("MarkItDown returned an invalid result")
    if len(markdown.encode("utf-8")) > MAX_MARKDOWN_BYTES:
        fail("extracted Markdown exceeds the 8 MiB safety limit")

    response = {
        "engine": "markitdown",
        "version": markitdown.__version__,
        "markdown": markdown,
    }
    sys.stdout.write(json.dumps(response, ensure_ascii=False, separators=(",", ":")))


if __name__ == "__main__":
    main()
