# NYX Voice Provider Notes

## Whisper STT

NYX uses OpenAI Whisper as a local sidecar. The sidecar defaults to the Turkish language code `tr`, keeps model size configurable, and uses JSONL over stdin/stdout so the Rust runtime can enforce process boundaries, timeouts, and cancellation.

## Qwen3-TTS Turkish

The official Qwen3-TTS repository documents native support for ten languages and does not list Turkish. The selected Turkish-capable checkpoint is `hcfk/qwen3-tts-turkish`, which its model card describes as an experimental Turkish LoRA adaptation of a Qwen3-TTS 0.6B base, requiring the base model separately. NYX resolves the compatible public runnable base as `Qwen/Qwen3-TTS-12Hz-0.6B-Base`. The model card says it can synthesize understandable Turkish but is not production-ready and must not be used for impersonation, fraud, non-consensual voice cloning, or misleading synthetic media.

NYX therefore exposes this checkpoint through a configurable provider interface, marks it as experimental in the UI and README, and does not claim native official Turkish support. A production deployment should replace it with a verified Turkish-capable checkpoint once available and preserve the same provider contract.

## Verified upstream inference and local artifacts

Sources: https://raw.githubusercontent.com/gokbilge/qwen3-tts-turkish/master/inference.py and https://github.com/QwenLM/Qwen3-TTS

The Turkish adaptation's upstream inference script loads a Qwen3-TTS base model, attaches a PEFT LoRA adapter, registers Turkish language id 2072, normalizes Turkish numbers, applies the documented character-by-character G2P schema, generates in non-streaming mode, and decodes with the speech tokenizer at 24 kHz. NYX's Qwen sidecar follows this workflow.

The official base checkpoint used locally is `Qwen/Qwen3-TTS-12Hz-0.6B-Base`; its model artifacts include a roughly 1.83 GB main model file and a roughly 682 MB speech-tokenizer file. The Turkish adapter's final local artifacts include `adapter_model.safetensors`, `code_predictor.pt`, and `adapter_config.json`. Model weights remain ignored by Git and are installed through the reproducible download script.
