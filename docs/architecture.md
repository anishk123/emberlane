# Architecture

Emberlane is a local-first LLM gateway with one implemented cloud backend: AWS.

## Local Path

```text
CLI / HTTP / MCP
  -> Emberlane gateway
  -> Ollama or echo runtime
  -> chat or chat-with-file response
```

Local mode does not require AWS, Terraform, S3, or cloud credentials.

## AWS Path

```text
Client
  -> Emberlane local gateway or Lambda WakeBridge
  -> AWS Auto Scaling Group
  -> ALB stable base_url
  -> OSS LLM runtime
  -> OpenAI-compatible response
```

Terraform creates the AWS deployment pack. Emberlane renders `terraform.tfvars.json` from model profiles and cost modes.

- `economy` wakes from `0 -> 1` on demand.
- `balanced` starts at `1` and scales down after idle.
- `always-on` stays at `1`.

## Future Cloud Shape

The code has a small cloud backend seam so GCP and Azure can be added later. They are not implemented today.
