# Emberlane MCP Support

MCP stdio is one of Emberlane's primary supported interfaces for the public alpha. It is the recommended integration path for agent clients and developer tools.

Use MCP for:

- runtime listing and status
- chat
- file upload
- chat with uploaded `.txt` or `.md` files
- runtime wake and sleep

Run:

```sh
cargo run -- mcp
```

The server speaks newline-delimited JSON-RPC 2.0 over stdio. Logs go to stderr; stdout contains only JSON-RPC responses.

Supported methods:

- `initialize`
- `notifications/initialized`
- `tools/list`
- `tools/call`

Supported tools:

- `emberlane_list_runtimes`
- `emberlane_status`
- `emberlane_chat`
- `emberlane_upload_file`
- `emberlane_chat_file`
- `emberlane_wake`
- `emberlane_sleep`

Use the HTTP/OpenAI-compatible API for application integration and existing OpenAI-compatible clients. Use the CLI for deployment, benchmarking, diagnostics, and operations.
