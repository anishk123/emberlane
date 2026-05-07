# Emberlane v0.1 API

Success:

```json
{"ok": true, "data": {}}
```

Error:

```json
{"ok": false, "error": {"code": "invalid_request", "message": "...", "details": {}}}
```

Endpoints:

- `GET /healthz`
- `GET /v1/runtimes`
- `GET /v1/runtimes/:runtime_id/status`
- `POST /v1/runtimes/:runtime_id/wake`
- `POST /v1/runtimes/:runtime_id/sleep`
- `POST /v1/route/:runtime_id`
- `POST /v1/chat/:runtime_id`
- `POST /v1/files`
- `GET /v1/files/:file_id`
- `POST /v1/chat-file/:runtime_id/:file_id`
- `POST /v1/chat/completions`
- `POST /v1/openai/:runtime_id/chat/completions`

`/healthz` is unauthenticated. Other endpoints require `Authorization: Bearer <api_key>` if `api_key` is configured.
