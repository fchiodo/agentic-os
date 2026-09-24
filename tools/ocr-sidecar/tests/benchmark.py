"""Reproducible local benchmark for the packaged OCR sidecar.

This is intentionally opt-in: it requires a verified, already installed model
and never downloads anything. Results are emitted as JSON so a release run can
be archived without committing generated document output.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
DEFAULT_BINARY = ROOT / "src-tauri" / "binaries" / "ocr-sidecar-aarch64-apple-darwin"
FIXTURE_ROOT = ROOT / "tests" / "fixtures" / "ocr"


class Sidecar:
    def __init__(self, binary: Path, home: Path) -> None:
        environment = {
            "PATH": "/usr/bin:/bin",
            "HOME": str(home),
            "TMPDIR": str(home),
            "HF_HUB_OFFLINE": "1",
            "TRANSFORMERS_OFFLINE": "1",
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "TOKENIZERS_PARALLELISM": "false",
            "LC_ALL": "C.UTF-8",
        }
        self.process = subprocess.Popen(
            [str(binary)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
            env=environment,
        )
        self.peak_rss_bytes = 0
        self._sampling = True
        self._sampler = threading.Thread(target=self._sample_memory, daemon=True)
        self._sampler.start()

    def _sample_memory(self) -> None:
        while self._sampling and self.process.poll() is None:
            try:
                value = subprocess.check_output(
                    ["/bin/ps", "-o", "rss=", "-p", str(self.process.pid)],
                    text=True,
                    stderr=subprocess.DEVNULL,
                ).strip()
                if value:
                    self.peak_rss_bytes = max(self.peak_rss_bytes, int(value) * 1024)
            except (OSError, subprocess.SubprocessError, ValueError):
                pass
            time.sleep(0.2)

    def request(self, payload: dict[str, Any], timeout_seconds: float = 1_800) -> tuple[dict[str, Any], float]:
        if self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("Sidecar pipes are unavailable")
        started = time.perf_counter()
        self.process.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        deadline = started + timeout_seconds
        while time.perf_counter() < deadline:
            line = self.process.stdout.readline()
            if not line:
                stderr = self.process.stderr.read() if self.process.stderr else ""
                raise RuntimeError(f"Sidecar exited before responding: {stderr.strip()}")
            response = json.loads(line)
            if response.get("requestId") != payload["requestId"]:
                continue
            if response.get("type") == "error":
                raise RuntimeError(f"{response.get('code')}: {response.get('message')}")
            if response.get("type") in {"completed", "health", "model-loaded", "shutdown"}:
                return response, (time.perf_counter() - started) * 1_000
        raise TimeoutError(f"Sidecar request timed out after {timeout_seconds:.0f}s")

    def close(self) -> None:
        try:
            if self.process.poll() is None:
                self.request({"protocolVersion": 1, "requestId": "shutdown", "command": "shutdown"}, 20)
        finally:
            if self.process.poll() is None:
                self.process.kill()
            self.process.wait(timeout=20)
            self._sampling = False
            self._sampler.join(timeout=1)


def conversion_request(
    request_id: str,
    fixture: dict[str, Any],
    model: Path,
    working_directory: Path,
) -> dict[str, Any]:
    expected_mode = fixture["expectedMode"]
    mode = "force-ocr" if expected_mode == "ocr" else "automatic"
    return {
        "protocolVersion": 1,
        "requestId": request_id,
        "command": "convert",
        "jobId": request_id,
        "inputPath": str((FIXTURE_ROOT / fixture["path"]).resolve()),
        "workingDirectory": str(working_directory.resolve()),
        "modelPath": str(model.resolve()),
        "processingMode": mode,
        "maxTokens": 2_048,
    }


def result_record(fixture: dict[str, Any], response: dict[str, Any], elapsed_ms: float) -> dict[str, Any]:
    result = response["result"]
    pages = int(result["totalPages"])
    return {
        "dataset": fixture["id"],
        "fixture": fixture["path"],
        "elapsedMs": round(elapsed_ms, 1),
        "secondsPerPage": round(elapsed_ms / 1_000 / max(pages, 1), 3),
        "pages": pages,
        "pagesDigital": result["pagesDigital"],
        "pagesOcr": result["pagesOcr"],
        "processingMode": result["processingMode"],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Benchmark the packaged local OCR sidecar")
    parser.add_argument("--model", default=os.environ.get("AGENTIC_OS_OCR_MODEL"))
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise SystemExit("The v1 OCR benchmark requires macOS on Apple Silicon")
    if not arguments.model:
        raise SystemExit("Pass --model or set AGENTIC_OS_OCR_MODEL to a verified model directory")
    model = Path(arguments.model).expanduser().resolve()
    binary = arguments.binary.expanduser().resolve()
    if not model.is_dir():
        raise SystemExit(f"Model directory not found: {model}")
    if not binary.is_file():
        raise SystemExit(f"Sidecar not found: {binary}; run pnpm prepare:ocr")

    manifest = json.loads((FIXTURE_ROOT / "manifest.json").read_text())
    fixtures = [fixture for fixture in manifest["fixtures"] if fixture["id"] in {"A", "B", "C", "D", "E"}]
    report: dict[str, Any] = {
        "schemaVersion": 1,
        "generatedAtUtc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "machine": platform.platform(),
        "architecture": platform.machine(),
        "protocolVersion": 1,
        "cold": [],
        "warm": [],
    }

    with tempfile.TemporaryDirectory(prefix="agentic-os-ocr-benchmark-") as temp:
        temp_root = Path(temp)
        cold_peak = 0
        for fixture in fixtures:
            working = temp_root / f"cold-{fixture['id']}"
            working.mkdir()
            sidecar = Sidecar(binary, temp_root)
            try:
                response, elapsed = sidecar.request(
                    conversion_request(f"cold-{fixture['id']}", fixture, model, working)
                )
                report["cold"].append(result_record(fixture, response, elapsed))
            finally:
                sidecar.close()
                cold_peak = max(cold_peak, sidecar.peak_rss_bytes)
        report["coldPeakRssBytes"] = cold_peak

        sidecar = Sidecar(binary, temp_root)
        try:
            _, model_load_ms = sidecar.request(
                {
                    "protocolVersion": 1,
                    "requestId": "warm-load-model",
                    "command": "load-model",
                    "modelPath": str(model),
                }
            )
            report["modelLoadMs"] = round(model_load_ms, 1)
            for fixture in fixtures:
                working = temp_root / f"warm-{fixture['id']}"
                working.mkdir()
                response, elapsed = sidecar.request(
                    conversion_request(f"warm-{fixture['id']}", fixture, model, working)
                )
                report["warm"].append(result_record(fixture, response, elapsed))
        finally:
            sidecar.close()
            report["warmPeakRssBytes"] = sidecar.peak_rss_bytes

    rendered = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    if arguments.output:
        arguments.output.parent.mkdir(parents=True, exist_ok=True)
        arguments.output.write_text(rendered)
    print(rendered, end="")


if __name__ == "__main__":
    main()
