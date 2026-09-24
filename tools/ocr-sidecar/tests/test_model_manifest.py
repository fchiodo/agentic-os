from __future__ import annotations

import hashlib
import json
import os
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "model-manifest.json"


class ModelManifestTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.manifest = json.loads(MANIFEST_PATH.read_text(encoding="utf-8"))
        cls.model = cls.manifest["models"]["paddleocr-vl"]

    def test_manifest_has_pinned_secure_source(self) -> None:
        self.assertEqual(self.manifest["schemaVersion"], 2)
        self.assertEqual(self.model["architecture"], "aarch64-apple-darwin")
        self.assertEqual(self.model["protocolVersion"], 1)
        self.assertRegex(self.model["revision"], r"^[0-9a-f]{40}$")
        sources = self.model["download"]["sources"]
        self.assertGreaterEqual(len(sources), 2)
        self.assertEqual(sources[0]["id"], "github-agentic-os-release")
        self.assertIn("github.com/fchiodo/agentic-os/releases/download/", sources[0]["baseUrl"])
        self.assertEqual(len({source["id"] for source in sources}), len(sources))
        for source in sources:
            self.assertEqual(source["scheme"], "https")
            self.assertTrue(source["baseUrl"].startswith("https://"))
        hugging_face = next(
            source for source in sources if source["id"] == "huggingface-paddlepaddle"
        )
        self.assertIn(self.model["revision"], hugging_face["baseUrl"])

    def test_declared_total_matches_file_inventory(self) -> None:
        total = sum(file["sizeBytes"] for file in self.model["files"])
        self.assertEqual(total, self.model["downloadSizeBytes"])
        self.assertEqual(len({file["path"] for file in self.model["files"]}), len(self.model["files"]))

    def test_file_entries_are_safe_and_hash_pinned(self) -> None:
        for file in self.model["files"]:
            path = Path(file["path"])
            self.assertEqual(path.name, file["path"])
            self.assertNotIn("..", path.parts)
            self.assertGreater(file["sizeBytes"], 0)
            self.assertTrue(re.fullmatch(r"[0-9a-f]{64}", file["sha256"]))

    @unittest.skipUnless(os.environ.get("AGENTIC_OS_OCR_MODEL"), "real model path not provided")
    def test_installed_snapshot_matches_every_hash(self) -> None:
        model_root = Path(os.environ["AGENTIC_OS_OCR_MODEL"]).resolve(strict=True)
        for expected in self.model["files"]:
            candidate = model_root / expected["path"]
            self.assertTrue(candidate.is_file(), expected["path"])
            self.assertEqual(candidate.stat().st_size, expected["sizeBytes"], expected["path"])
            hasher = hashlib.sha256()
            with candidate.open("rb") as stream:
                for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                    hasher.update(chunk)
            digest = hasher.hexdigest()
            self.assertEqual(digest, expected["sha256"], expected["path"])


if __name__ == "__main__":
    unittest.main()
