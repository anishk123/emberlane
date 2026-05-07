# Cleanup Notes

Emberlane is scoped to:

- one-prompt chat
- chat with uploaded `.txt` or `.md` files
- local Ollama
- local echo demos/tests
- HTTP API
- MCP stdio for core tools
- AWS deploy/chat/benchmark/cost-report/destroy

Unfinished RAG, search, workflow, plugin, dashboard, SDK, and multi-cloud examples should not be presented as working Emberlane features. Those belong outside the core gateway until they are real, central, and tested.

Useful future ideas live in `docs/roadmap/future-work.md`.

The active examples are:

- `examples/echo-runtime`
- `examples/simple-chat`

SDK directories should not exist in the active source tree until they are implemented, tested, documented, and supported.
