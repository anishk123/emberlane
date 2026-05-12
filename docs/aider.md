# Emberlane And Aider

Emberlane can expose an OpenAI-compatible endpoint that Aider can use directly.

## Recommended Setup

```bash
export OPENAI_API_BASE=http://<emberlane-endpoint>/v1
export OPENAI_API_KEY=<emberlane-api-key>
aider --openai-api-base "$OPENAI_API_BASE" --model openai/qwen3_4b_inf2_4k
```

## Notes

- Replace `<emberlane-endpoint>` with the deployed AWS endpoint or local HTTP endpoint.
- Use `openai/qwen3_4b_inf2_4k` for the public default coding profile.
- If you deploy a different profile, use the corresponding Emberlane model name prefixed with `openai/`.
- Emberlane serves both `GET /v1/models` and `POST /v1/chat/completions`, including streaming.
