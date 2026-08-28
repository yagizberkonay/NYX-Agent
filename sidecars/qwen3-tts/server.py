#!/usr/bin/env python3
"""NYX local Qwen3-TTS Turkish LoRA sidecar.

The upstream Turkish adaptation is a research checkpoint. It loads a Qwen3-TTS
base model plus a PEFT adapter, applies the documented Turkish G2P schema, and
returns one JSON response per JSONL request.
"""
from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path
from typing import Any

_MODEL_CACHE: dict[tuple[str, str, str], Any] = {}
TURKISH_LANG_ID = 2072

_ONES = ["", "bir", "iki", "üç", "dört", "beş", "altı", "yedi", "sekiz", "dokuz"]
_TENS = ["", "on", "yirmi", "otuz", "kırk", "elli", "altmış", "yetmiş", "seksen", "doksan"]
SCHEMA_D = {
    "ç": "ch", "Ç": "Ch", "ş": "sch", "Ş": "Sch", "ğ": "", "Ğ": "",
    "ü": "ue", "Ü": "Ue", "ö": "oe", "Ö": "Oe", "ı": "i", "c": "j", "C": "J",
}


def emit(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def _int_to_tr(number: int) -> str:
    if number < 0:
        return "eksi " + _int_to_tr(-number)
    if number == 0:
        return "sıfır"
    if number < 10:
        return _ONES[number]
    if number < 100:
        return (_TENS[number // 10] + (" " + _ONES[number % 10] if number % 10 else "")).strip()
    if number < 1000:
        hundreds = "bir yüz" if number // 100 == 1 else _ONES[number // 100] + " yüz"
        return (hundreds + (" " + _int_to_tr(number % 100) if number % 100 else "")).strip()
    if number < 1_000_000:
        thousands = "bin" if number // 1000 == 1 else _int_to_tr(number // 1000) + " bin"
        return (thousands + (" " + _int_to_tr(number % 1000) if number % 1000 else "")).strip()
    if number < 1_000_000_000:
        millions = _int_to_tr(number // 1_000_000) + " milyon"
        return (millions + (" " + _int_to_tr(number % 1_000_000) if number % 1_000_000 else "")).strip()
    return str(number)


def normalize_numbers(text: str) -> str:
    return re.sub(r"\d+", lambda match: _int_to_tr(int(match.group())), text)


def apply_schema_d(text: str) -> str:
    # Character-by-character replacement avoids cascading substitutions.
    return "".join(SCHEMA_D.get(character, character) for character in text)


def load_model(base_model: str, adapter_dir: str, device: str) -> Any:
    key = (base_model, adapter_dir, device)
    if key in _MODEL_CACHE:
        return _MODEL_CACHE[key]
    if not adapter_dir:
        raise RuntimeError(
            "NYX_QWEN_TTS_ADAPTER_DIR is not configured. Set it to the Turkish "
            "LoRA checkpoint directory; the official Qwen3-TTS weights do not include native Turkish."
        )
    adapter_path = Path(adapter_dir).expanduser().resolve()
    if not adapter_path.exists():
        raise RuntimeError(f"Turkish Qwen3-TTS adapter directory does not exist: {adapter_path}")
    try:
        import torch  # type: ignore
        from peft import PeftModel  # type: ignore
        from qwen_tts import Qwen3TTSModel  # type: ignore
    except ImportError as exc:
        raise RuntimeError(
            "Qwen3-TTS Turkish dependencies are not installed. Run: "
            "pip install -r sidecars/requirements.txt"
        ) from exc

    resolved_device = device
    if resolved_device == "auto":
        resolved_device = "cuda" if torch.cuda.is_available() else "cpu"
    dtype = torch.bfloat16 if resolved_device.startswith("cuda") else torch.float32
    wrapper = Qwen3TTSModel.from_pretrained(
        base_model,
        device_map=resolved_device,
        dtype=dtype,
    )
    wrapper.model.talker.model = PeftModel.from_pretrained(
        wrapper.model.talker.model,
        str(adapter_path),
    )
    wrapper.model.talker.model.eval()
    _MODEL_CACHE[key] = wrapper
    return wrapper


def synthesize(payload: dict[str, Any]) -> dict[str, Any]:
    text = str(payload.get("text", "")).strip()
    if not text:
        raise ValueError("text must not be empty")
    output_path = Path(str(payload.get("output_path", ""))).expanduser().resolve()
    if output_path.suffix.lower() != ".wav":
        raise ValueError("output_path must end with .wav")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    base_model = str(payload.get("base_model") or os.getenv("NYX_QWEN_TTS_BASE_MODEL", "Qwen/Qwen3-TTS-0.6B-Base"))
    adapter_dir = str(payload.get("adapter_dir") or os.getenv("NYX_QWEN_TTS_ADAPTER_DIR", ""))
    device = str(payload.get("device") or os.getenv("NYX_QWEN_TTS_DEVICE", "auto"))
    language = str(payload.get("language") or "Turkish")
    processed_text = apply_schema_d(normalize_numbers(text))
    wrapper = load_model(base_model, adapter_dir, device)

    try:
        import numpy as np  # type: ignore
        import soundfile as sf  # type: ignore
        import torch  # type: ignore
    except ImportError as exc:
        raise RuntimeError("numpy, soundfile, and torch are required for synthesis") from exc

    wrapper.model.config.talker_config.codec_language_id["turkish"] = TURKISH_LANG_ID
    wrapper.model.supported_languages = list(wrapper.model.supported_languages) + ["turkish"]
    input_ids = [wrapper.processor(
        text=f"<|im_start|>assistant\n{processed_text}<|im_end|>\n<|im_start|>assistant\n",
        return_tensors="pt",
    )["input_ids"].to(wrapper.device)]
    with torch.inference_mode():
        talker_codes, _ = wrapper.model.generate(
            input_ids=input_ids,
            languages=["turkish"],
            non_streaming_mode=True,
        )
        speech_tokenizer = wrapper.model.speech_tokenizer.model
        device_name = str(wrapper.device)
        codes_tensor = talker_codes[0].unsqueeze(0).to(device_name)
        wav = speech_tokenizer.decode(codes_tensor)
    wav_np = np.asarray(wav.audio_values[0].cpu().float().numpy())
    active = np.where(np.abs(wav_np) > 0.005)[0]
    if len(active) > 0:
        wav_np = wav_np[: active[-1] + int(0.15 * 24000)]
    sf.write(str(output_path), wav_np, 24000)
    return {
        "ok": True,
        "provider": "qwen3-tts-turkish",
        "base_model": base_model,
        "adapter_dir": str(Path(adapter_dir).expanduser()),
        "language": language,
        "output_path": str(output_path),
        "sample_rate": 24000,
        "experimental": True,
        "processed_text": processed_text,
    }


def _installed() -> bool:
    try:
        import peft  # type: ignore # noqa: F401
        import qwen_tts  # type: ignore # noqa: F401
        import soundfile  # type: ignore # noqa: F401
        return True
    except ImportError:
        return False


def main() -> None:
    for raw_line in sys.stdin:
        if not raw_line.strip():
            continue
        try:
            payload = json.loads(raw_line)
            action = payload.get("action", "synthesize")
            if action == "health":
                emit({
                    "ok": True,
                    "provider": "qwen3-tts-turkish",
                    "installed": _installed(),
                    "base_model": os.getenv("NYX_QWEN_TTS_BASE_MODEL", "Qwen/Qwen3-TTS-0.6B-Base"),
                    "adapter_configured": bool(os.getenv("NYX_QWEN_TTS_ADAPTER_DIR")),
                    "experimental": True,
                })
            elif action == "synthesize":
                emit(synthesize(payload))
            else:
                raise ValueError(f"unsupported action: {action}")
        except Exception as exc:
            emit({
                "ok": False,
                "provider": "qwen3-tts-turkish",
                "error": str(exc),
                "error_type": type(exc).__name__,
            })


if __name__ == "__main__":
    main()
