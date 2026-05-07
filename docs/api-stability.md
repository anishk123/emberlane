# API Stability

Emberlane v0.1 treats these as the useful stable surface:

- CLI: `init`, `serve`, `status`, `wake`, `sleep`, `chat`, `route`, `upload`, `chat-file`, `mcp`
- HTTP: health, runtimes, wake/sleep, route, chat, files, file-chat, non-streaming chat completions
- MCP: initialize, tools/list, tools/call and the eight v0.1 tools

Everything outside this surface is intentionally absent from v0.1.
