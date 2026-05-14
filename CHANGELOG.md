# Changelog

All notable changes to Emberlane will be documented here.

## v0.3.9

- Clarified the Qwen3.5 CUDA lineup and preserved both base and quantized choices.
- Fixed the Qwen3.5 2B quantized lane to match its compressed-tensors packaging.
- Refreshed the model picker and docs so the end-user labels are easier to read.

## v0.3.8

- Restored the Qwen2.5 Inf2 economy lane.
- Fixed Inf2 runtime model-name matching for chat requests.
- Updated the Inf2 smoke test to use the active profile model ID.

## Unreleased

- Prepared the repository for a public alpha release.
- Simplified licensing to MIT only.
- Hardened CI, contribution, security, issue, PR, and release-readiness documentation.

## v0.1.0-alpha

- Added local-first wake gateway behavior.
- Added deterministic echo runtime for demos and tests.
- Added local Ollama runtime support.
- Added CLI, HTTP API, SQLite state, file upload, and `.txt`/`.md` file-context chat.
- Added OpenAI-compatible non-streaming chat endpoint.
- Added MCP stdio tools for runtime listing, status, chat, upload, chat-file, wake, and sleep.
- Added AWS-first deployment direction with Terraform, ASG WakeBridge, Lambda WakeBridge, optional S3 artifact storage, CUDA/vLLM path, and experimental Inf2/Neuron runtime pack.
