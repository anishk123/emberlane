# Roadmap

Emberlane is a public alpha. The roadmap is intentionally narrow so the project stays useful and honest.

## v0.1 Local Foundation

- Local echo runtime.
- Local Ollama runtime.
- HTTP chat and OpenAI-compatible chat.
- File upload and `.txt`/`.md` chat-with-file.
- MCP stdio tools for the core workflow.

## v0.2 AWS Terraform And CUDA/vLLM

- Terraform deployment pack for AWS dev/test inference.
- ASG wake/sleep behavior with ALB stable endpoint.
- Lambda WakeBridge for always-on request routing.
- CUDA/vLLM runtime path as the recommended first AWS deployment route.

## v0.3 Benchmark And Cost Reporting

- Live AWS integration-test harness.
- Benchmark reports with real timing fields.
- Cost reporting that refuses fake savings when pricing inputs are missing.
- Safer diagnostics and cleanup flows for billable AWS resources.
- Planned Python and TypeScript SDK design, but no supported SDK release until tests and docs exist.

## v0.4 Inf2/Neuron Experimental

- Inf2 runtime pack for vLLM/Neuron experiments.
- Model profile rendering for small Llama and Qwen profiles.
- Neuron cache/artifact documentation.
- Clear validation records before any model is marked validated.

## Future Backends

- GCP backend: future only.
- Azure backend: future only.
- Multi-cloud abstractions should stay thin until AWS is proven end-to-end.
