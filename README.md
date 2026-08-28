# NYX-Agent

NYX is a local-first, BYOK desktop computer agent. This repository contains a production-oriented Rust core, a Tauri/React desktop UI, secure workspace-scoped tools, and optional local voice sidecars.

## Current vertical slice

The first slice supports a real, bounded workflow over a selected workspace: inspect files, search content, run a command with explicit target metadata, read Git state, and stream operational events to the UI. The Rust runtime owns policy, tool execution, cancellation, and audit-safe event generation. The UI never receives raw secrets or direct host authority.

## Architecture

The desktop application is split into a React/TypeScript UI, a thin Tauri bridge, and a Rust runtime. The runtime is organized into core value types, security policy, tools, filesystem access, and the agent state machine. Voice integrations are isolated as local sidecars so that model runtimes do not become part of the privileged UI process.

## Local development

Install Node.js 22+, pnpm, Rust stable, and the platform prerequisites for Tauri. Then run:

```bash
pnpm install
pnpm dev
cargo test --workspace
```

The frontend can be built independently with `pnpm build`. The Rust workspace is tested with `cargo test --workspace`.

## Voice sidecars

Whisper is the default local speech-to-text provider. The sidecar accepts an audio path on stdin as JSON and returns a deterministic JSON response. The `tr` language is the default, and the model is configurable through `NYX_WHISPER_MODEL`.

Qwen3-TTS is exposed through the same provider contract. The official Qwen3-TTS repository currently documents ten native languages and does not list Turkish; therefore Turkish must be supplied by a verified Turkish-capable checkpoint or adapter configured through `NYX_QWEN_TTS_MODEL`. The application keeps this model configurable and does not silently claim that the official checkpoint is native Turkish.

Install optional providers in an isolated Python environment:

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r sidecars/requirements.txt
```

Whisper also requires `ffmpeg` on the host. Model weights are downloaded by the provider runtime on first use and are intentionally excluded from Git.

## Security defaults

Workspace access is scoped to the configured root. Path traversal and symlink escapes are rejected. Shell execution is disabled by default in the UI until a user-approved policy is supplied. Tool inputs are schema-validated, time-bounded, cancellable, and represented with structured audit events. Secrets belong in the operating system credential store or deployment secret manager; they must never be committed to this repository.

## Deployment

The desktop application is distributed as a signed installer. The Vercel deployment is a static product/demo surface for the UI and documentation; it does not expose host controls, local secrets, or privileged runtime APIs. The local Tauri binary remains the only component that can operate the user's machine.

## License

Apache-2.0
