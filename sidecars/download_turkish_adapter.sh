#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODEL_DIR="${NYX_QWEN_TTS_ADAPTER_ROOT:-${ROOT_DIR}/models/qwen3-tts-turkish}"
mkdir -p "${MODEL_DIR}"

if [[ ! -x "${ROOT_DIR}/.venv/bin/hf" ]]; then
  echo "Run sidecars/install.sh first." >&2
  exit 2
fi

"${ROOT_DIR}/.venv/bin/hf" download hcfk/qwen3-tts-turkish \
  adapter/final/adapter_model.safetensors \
  adapter/final/code_predictor.pt \
  adapter/final/adapter_config.json \
  --local-dir "${MODEL_DIR}"

printf 'NYX_QWEN_TTS_ADAPTER_DIR=%s\n' "${MODEL_DIR}/adapter/final"
