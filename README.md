# NYX-Agent

NYX is a local-first, BYOK desktop computer agent. This repository contains a production-oriented Rust core, a Tauri/React desktop UI, secure workspace tools, cross-platform host-control tools, and optional local voice sidecars.

## Current capability surface

The runtime now exposes a schema-driven tool registry containing workspace filesystem tools and host-control tools. The host layer supports opening allowlisted desktop applications and web services, opening validated HTTP(S) URLs, inspecting running processes, executing bounded commands inside the active workspace, controlling local media players, and sending desktop notifications. Every invocation carries target metadata, timeout behavior, cancellation, and structured audit-safe results.

The current vertical slice can inspect and search a workspace, write inside the workspace, launch common applications such as Spotify/YouTube/browser/terminal/files/VS Code where the host provides the corresponding command, list processes, control media through `playerctl`/AppleScript where available, and execute validated workspace-local shell commands. The agent runtime exposes these descriptors through the Tauri bridge and the React UI.

## Architecture

The desktop application is split into a React/TypeScript UI, a thin Tauri bridge, and a Rust runtime. The runtime is organized into core value types, security policy, tools, filesystem access, host control, voice providers, and the agent state machine. Voice integrations are isolated as local sidecars so that model runtimes do not become part of the privileged UI process.

## Local development

Install Node.js 22+, pnpm, Rust stable, and the platform prerequisites for Tauri. Then run:

```bash
cd NYX-Agent
pnpm --dir apps/desktop install
pnpm --dir apps/desktop tauri:dev
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The production UI build is created with `pnpm --dir apps/desktop build`. The real desktop agent is launched with `pnpm --dir apps/desktop tauri:dev`; running only `pnpm --dir apps/desktop dev` starts the browser UI without the privileged Tauri runtime. A local desktop installer is produced with `pnpm --dir apps/desktop tauri:build` after the platform WebView dependencies are installed. Windows NSIS/MSI installers are built by `.github/workflows/release-windows.yml` on a Windows GitHub Actions runner when a `v*` tag is pushed or the workflow is manually dispatched.

## Autonomous mode

Set `NYX_AUTONOMY_MODE=autonomous` in the local environment to suppress interactive approval prompts for in-workspace mutations and explicitly supported host actions. This is not an unrestricted root shell: workspace boundary checks, path traversal protection, URL validation, dangerous command-pattern blocking, process timeouts, cancellation, and audit events remain active. The default remains `manual`.

Never commit API keys, OAuth tokens, cookies, browser profiles, or model weights. Store credentials in the operating system credential store or environment manager and keep `.env` local.

## Voice sidecars

Whisper is the local speech-to-text provider. It accepts an audio path through a JSONL stdin protocol and returns a deterministic JSON response. Turkish (`tr`) is the default language and the model is configurable through `NYX_WHISPER_MODEL`.

Qwen3-TTS is exposed through the same provider contract. The official Qwen3-TTS checkpoint does not list Turkish among its native languages, so NYX uses the experimental `hcfk/qwen3-tts-turkish` adapter with the compatible `Qwen/Qwen3-TTS-12Hz-0.6B-Base` checkpoint. The adapter is marked experimental and is not represented as native official Turkish support.

Install the optional providers in an isolated environment:

```bash
./sidecars/install.sh
./sidecars/download_turkish_adapter.sh
```

Set the local paths when using downloaded weights:

```bash
export NYX_QWEN_TTS_BASE_MODEL="$PWD/models/qwen3-tts-base"
export NYX_QWEN_TTS_ADAPTER_DIR="$PWD/models/qwen3-tts-turkish/adapter/final"
```

Whisper requires `ffmpeg`; Qwen3-TTS checks for `sox` as well. Model weights are intentionally excluded from Git. The sidecars report health independently, and the Rust provider enforces process boundaries, cancellation, and timeouts.

## Integration roadmap

Calendar, email, WordPress messaging, CRM, lead research, deep research, browser automation, project scaffolding, Spotify/YouTube API control, proactive reminders, and durable customer follow-up require provider adapters and user-owned credentials. The registry and policy contracts are designed for these additions, but they are not silently claimed as completed merely because a generic descriptor exists. Connectors should be enabled only for the services the user actually uses, with provider-specific scopes, rate limits, idempotency keys, audit records, and explicit opt-out controls.

## Desktop versus web

The Vercel deployment is a static product/demo surface for the UI. It does not expose host controls, local secrets, browser sessions, or privileged runtime APIs. For real PC control, install and launch the Tauri desktop application; the local binary is the component that can operate the user's machine. GitHub Actions validates Rust formatting, Clippy, workspace tests, frontend build, and Python sidecar syntax, while the Windows release workflow produces NSIS/MSI installers.

## Deployment

Pushes to GitHub update the source repository. The Vercel deployment is intentionally limited to the non-privileged web surface; it must not be treated as the desktop agent backend.

## License

Apache-2.0
