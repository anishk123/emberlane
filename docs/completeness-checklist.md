# Completeness Checklist

This checklist describes what is supported in the public alpha and what is intentionally not supported yet.

## Supported And Tested

- CLI local echo runtime.
- CLI Ollama runtime when Ollama is installed and the configured model is pulled.
- File upload.
- Chat-file for `.txt` and `.md`.
- MCP `tools/list`.
- MCP `tools/call`.
- HTTP `GET /healthz`.
- HTTP `POST /v1/chat/:runtime_id`.
- OpenAI-compatible `POST /v1/chat/completions`.
- AWS Terraform deploy path.
- AWS benchmark and cost-report commands.

## Supported Interfaces

- CLI for setup, deployment, benchmark, cost reporting, cleanup, and diagnostics.
- MCP stdio for agent and developer-tool integration.
- HTTP/OpenAI-compatible API for applications and existing clients.

## Not Supported Yet

- Python SDK.
- TypeScript SDK.
- GCP backend.
- Azure backend.
- Production UI.
- Managed hosted service.

## Notes

SDK directories are intentionally absent from the active source tree until they are implemented, tested, and documented. Future SDK plans live in `docs/roadmap/future-work.md`.
