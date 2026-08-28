#!/usr/bin/env python3
"""NYX local Whisper sidecar.

Protocol: one JSON object per stdin line, one JSON result per stdout line.
Example input: {"action":"transcribe","audio_path":"/tmp/input.wav","language":"tr","model":"base"}
"""
from __future__ import annotations

import json
import os
import sys
import traceback
from pathlib import Path
from typing import Any

_MODEL_CACHE: dict[str, Any] = {}


def emit(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def load_whisper(model_name: str) -> Any:
    if model_name not in _MODEL_CACHE:
        try:
            import whisper  # type: ignore
        except ImportError as exc:
            raise RuntimeError(
                "Whisper is not installed. Run: pip install -r sidecars/requirements.txt"
            ) from exc
        _MODEL_CACHE[model_name] = whisper.load_model(model_name)
    return _MODEL_CACHE[model_name]


def transcribe(payload: dict[str, Any]) -> dict[str, Any]:
    audio_path = Path(str(payload.get("audio_path", ""))).expanduser().resolve()
    if not audio_path.is_file():
        raise ValueError(f"audio file does not exist: {audio_path}")
    model_name = str(payload.get("model") or os.getenv("NYX_WHISPER_MODEL", "base"))
    language = str(payload.get("language") or os.getenv("NYX_WHISPER_LANGUAGE", "tr"))
    task = str(payload.get("task") or "transcribe")
    model = load_whisper(model_name)
    result = model.transcribe(
        str(audio_path),
        language=language,
        task=task,
        fp16=False,
        verbose=False,
    )
    segments = [
        {
            "start": float(segment.get("start", 0.0)),
            "end": float(segment.get("end", 0.0)),
            "text": str(segment.get("text", "")).strip(),
        }
        for segment in result.get("segments", [])
    ]
    return {
        "ok": True,
        "provider": "whisper",
        "model": model_name,
        "language": result.get("language", language),
        "text": str(result.get("text", "")).strip(),
        "segments": segments,
    }


def main() -> None:
    for raw_line in sys.stdin:
        if not raw_line.strip():
            continue
        try:
            payload = json.loads(raw_line)
            action = payload.get("action", "transcribe")
            if action == "health":
                emit({"ok": True, "provider": "whisper", "installed": _whisper_installed()})
            elif action == "transcribe":
                emit(transcribe(payload))
            else:
                raise ValueError(f"unsupported action: {action}")
        except Exception as exc:  # keep the JSONL process alive after one bad request
            emit({
                "ok": False,
                "provider": "whisper",
                "error": str(exc),
                "error_type": type(exc).__name__,
            })


def _whisper_installed() -> bool:
    try:
        import whisper  # type: ignore # noqa: F401
        return True
    except ImportError:
        return False


if __name__ == "__main__":
    main()
