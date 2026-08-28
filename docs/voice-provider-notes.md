# NYX Voice Provider Notes

## Whisper STT

NYX uses OpenAI Whisper as a local sidecar. The sidecar defaults to the Turkish language code `tr`, keeps model size configurable, and uses JSONL over stdin/stdout so the Rust runtime can enforce process boundaries, timeouts, and cancellation.

## Qwen3-TTS Turkish

The official Qwen3-TTS repository documents native support for ten languages and does not list Turkish. The selected Turkish-capable checkpoint is `hcfk/qwen3-tts-turkish`, which its model card describes as an experimental Turkish LoRA adaptation of `Qwen/Qwen3-TTS-0.6B-Base`, requiring the base model separately. The model card says it can synthesize understandable Turkish but is not production-ready and must not be used for impersonation, fraud, non-consensual voice cloning, or misleading synthetic media.

NYX therefore exposes this checkpoint through a configurable provider interface, marks it as experimental in the UI and README, and does not claim native official Turkish support. A production deployment should replace it with a verified Turkish-capable checkpoint once available and preserve the same provider contract.
