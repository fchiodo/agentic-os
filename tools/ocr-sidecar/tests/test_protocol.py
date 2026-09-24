from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path


SIDECAR_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SIDECAR_ROOT))

from protocol import PROTOCOL_VERSION, ProtocolError, error_message, parse_request  # noqa: E402


class ProtocolTests(unittest.TestCase):
    def test_parses_versioned_command(self) -> None:
        request = parse_request(
            json.dumps(
                {
                    "protocolVersion": PROTOCOL_VERSION,
                    "requestId": "health-1",
                    "command": "health",
                }
            )
        )
        self.assertEqual(request["command"], "health")

    def test_rejects_invalid_json_with_machine_error(self) -> None:
        with self.assertRaises(ProtocolError) as context:
            parse_request("not-json")
        self.assertEqual(error_message(context.exception)["code"], "INVALID_JSON")

    def test_rejects_incompatible_protocol(self) -> None:
        with self.assertRaises(ProtocolError) as context:
            parse_request('{"protocolVersion":2,"requestId":"x","command":"health"}')
        self.assertEqual(context.exception.code, "PROTOCOL_MISMATCH")
        self.assertEqual(context.exception.request_id, "x")

    def test_rejects_non_string_command(self) -> None:
        with self.assertRaises(ProtocolError) as context:
            parse_request('{"protocolVersion":1,"command":42}')
        self.assertEqual(context.exception.code, "INVALID_REQUEST")


if __name__ == "__main__":
    unittest.main()
