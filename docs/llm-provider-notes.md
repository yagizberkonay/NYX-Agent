# NYX LLM Provider Notes

Catalog checked on 2026-08-28 through the configured OpenAI-compatible proxy. Available model IDs were: `gpt-5-nano`, `gpt-5-mini`, `gpt-5`, `gpt-5.5`, `gemini-3-flash-preview`, and `gemini-3.1-pro-preview`.

Recommended routing for NYX is `gpt-5-mini` for routine classification, extraction, summaries, and low-cost planner steps; `gpt-5` for code generation and normal agentic reasoning; and `gpt-5.5` or `gemini-3.1-pro-preview` for the hardest deep-research synthesis. GPT-5-series calls use `max_completion_tokens` when reasoning is enabled. Gemini calls use `max_tokens` rather than `max_completion_tokens`.

The desktop runtime should keep provider keys local and use structured JSON tool plans. Generated tool names and arguments must be validated against the registry before execution, with workspace scope, timeout, cancellation, audit, and autonomy policy enforcement applied after model output.
