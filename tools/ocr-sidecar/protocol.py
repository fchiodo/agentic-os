"""Versioned JSON Lines protocol primitives for the local OCR sidecar."""

from __future__ import annotations

import json
from typing import Any


PROTOCOL_VERSION = 1


class ProtocolError(ValueError):
    """A request could not be accepted at the JSONL trust boundary."""

    def __init__(self, code: str, message: str, request_id: object = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.request_id = request_id


def parse_request(raw_line: str) -> dict[str, Any]:
    try:
        request = json.loads(raw_line)
    except json.JSONDecodeError as error:
        raise ProtocolError("INVALID_JSON", "Request is not valid JSON") from error

    if not isinstance(request, dict):
        raise ProtocolError("INVALID_REQUEST", "Request must be an object")

    request_id = request.get("requestId")
    if request.get("protocolVersion") != PROTOCOL_VERSION:
        raise ProtocolError(
            "PROTOCOL_MISMATCH",
            "Unsupported protocol version",
            request_id,
        )
    if not isinstance(request.get("command"), str):
        raise ProtocolError("INVALID_REQUEST", "command must be a string", request_id)
    return request


def message(message_type: str, request_id: object, **payload: object) -> dict[str, object]:
    return {
        "protocolVersion": PROTOCOL_VERSION,
        "type": message_type,
        "requestId": request_id,
        **payload,
    }


def error_message(error: ProtocolError) -> dict[str, object]:
    return message(
        "error",
        error.request_id,
        code=error.code,
        message=error.message,
    )
