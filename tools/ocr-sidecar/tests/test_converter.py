from __future__ import annotations

import tempfile
import sys
import unittest
from pathlib import Path

from PIL import Image


SIDECAR_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SIDECAR_ROOT))

from converter import _save_normalized_image, convert_document  # noqa: E402
from engine import EngineError  # noqa: E402


class FakeEngine:
    def convert_image(
        self,
        image_path: object,
        model_path: object,
        instruction: object,
        max_tokens: object,
    ) -> tuple[str, int]:
        self.last_image = str(image_path)
        return "# Synthetic invoice\n\nTotal: €42.00", 12


class ConverterTests(unittest.TestCase):
    def test_converts_single_image_and_emits_real_stages(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "scan.png"
            Image.new("RGB", (320, 180), "white").save(image)
            work = root / "work"
            work.mkdir()
            events: list[tuple[str, int | None, int | None, str]] = []
            result = convert_document(
                {
                    "inputPath": str(image),
                    "workingDirectory": str(work),
                    "processingMode": "automatic",
                    "modelPath": str(root / "model"),
                    "maxTokens": 512,
                },
                FakeEngine(),
                lambda stage, page, total, label: events.append((stage, page, total, label)),
            )
            self.assertEqual(result["pagesOcr"], 1)
            self.assertEqual(result["pages"][0]["text"], "# Synthetic invoice\n\nTotal: €42.00")
            self.assertEqual([event[0] for event in events], ["rendering", "ocr"])
            self.assertTrue((work / "page-001.png").is_file())

    def test_rejects_unsupported_file(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "sheet.xlsx"
            source.write_bytes(b"not-a-document")
            work = root / "work"
            work.mkdir()
            with self.assertRaises(EngineError) as context:
                convert_document(
                    {"inputPath": str(source), "workingDirectory": str(work)},
                    FakeEngine(),
                    lambda *_: None,
                )
            self.assertEqual(context.exception.code, "UNSUPPORTED_FILE")

    def test_digital_only_rejects_image(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / "scan.jpg"
            Image.new("RGB", (64, 64), "white").save(image)
            work = root / "work"
            work.mkdir()
            with self.assertRaises(EngineError) as context:
                convert_document(
                    {
                        "inputPath": str(image),
                        "workingDirectory": str(work),
                        "processingMode": "digital-only",
                    },
                    FakeEngine(),
                    lambda *_: None,
                )
            self.assertEqual(context.exception.code, "UNSUPPORTED_FILE")

    def test_converts_scanned_multipage_pdf_incrementally(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pdf = root / "scan.pdf"
            pages = [Image.new("RGB", (180, 120), color) for color in ("white", "ivory")]
            pages[0].save(pdf, "PDF", save_all=True, append_images=pages[1:])
            work = root / "work"
            work.mkdir()
            result = convert_document(
                {
                    "inputPath": str(pdf),
                    "workingDirectory": str(work),
                    "processingMode": "force-ocr",
                    "modelPath": str(root / "model"),
                    "maxTokens": 512,
                },
                FakeEngine(),
                lambda *_: None,
            )
            self.assertEqual(result["totalPages"], 2)
            self.assertEqual(result["pagesOcr"], 2)
            self.assertTrue((work / "page-001.png").is_file())
            self.assertTrue((work / "page-002.png").is_file())

    def test_normalizes_all_supported_exif_orientations(self) -> None:
        corrections = {1: 0, 6: 90, 3: 180, 8: 270}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for orientation, expected_correction in corrections.items():
                source = root / f"orientation-{orientation}.jpg"
                destination = root / f"normalized-{orientation}.png"
                image = Image.new("RGB", (80, 40), "white")
                exif = image.getexif()
                exif[274] = orientation
                image.save(source, exif=exif)

                correction = _save_normalized_image(source, destination)
                self.assertEqual(correction, expected_correction)
                with Image.open(destination) as normalized:
                    expected_size = (40, 80) if orientation in {6, 8} else (80, 40)
                    self.assertEqual(normalized.size, expected_size)


if __name__ == "__main__":
    unittest.main()
