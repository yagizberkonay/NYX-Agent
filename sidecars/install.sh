#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENV_DIR="${NYX_VOICE_VENV:-${ROOT_DIR}/.venv}"

python3 -m venv "${VENV_DIR}"
# shellcheck disable=SC1091
source "${VENV_DIR}/bin/activate"
python -m pip install --upgrade pip wheel
python -m pip install -r "${ROOT_DIR}/sidecars/requirements.txt"

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg is required by Whisper. Install it with your OS package manager." >&2
  exit 2
fi

cat <<EOF
Voice providers installed in ${VENV_DIR}.
Whisper sidecar: ${ROOT_DIR}/sidecars/whisper/server.py
Qwen3-TTS Turkish sidecar: ${ROOT_DIR}/sidecars/qwen3-tts/server.py

For Turkish Qwen3-TTS, set NYX_QWEN_TTS_ADAPTER_DIR to the downloaded adapter directory.
The adapter is experimental and requires its Qwen3-TTS base model.
EOF
