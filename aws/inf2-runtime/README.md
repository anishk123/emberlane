# Inf2 Runtime Pack

The Emberlane Inf2 Runtime Pack turns an AWS Inf2 EC2 instance into a wakeable OpenAI-compatible LLM runtime.

## Public Inf2 Profiles

The public Inf2 targets are centered on Qwen3:

- `Qwen/Qwen3-4B-Instruct-2507` on `inf2.xlarge`
- `Qwen/Qwen3-8B` on `inf2.8xlarge`, with `inf2.24xlarge` as the larger-memory fallback

Legacy Qwen2.5 Inf2 compatibility profiles remain hidden and only appear with `--experimental` or `--show-hidden`.

## Quick Start

```sh
cd aws/inf2-runtime
docker build -f Dockerfile.neuron -t emberlane-inf2-neuron .
```

For a manual boot on an Inf2 EC2 instance:

1. Pick an AWS Neuron Deep Learning AMI for Ubuntu.
2. Use `inf2.xlarge` for `qwen3_4b_inf2_4k`.
3. Use `inf2.8xlarge` for `qwen3_8b_inf2_32k`; move to `inf2.24xlarge` if the 32K test needs more accelerator memory.
4. Attach at least 100 GB gp3.
5. Ensure `/dev/neuron0` exists.
6. Copy `aws/inf2-runtime` to `/opt/emberlane/inf2-runtime`.
7. Create `/etc/emberlane/inf2.env`.
8. Run `sudo /opt/emberlane/inf2-runtime/bootstrap.sh`.

## Model Weights

```sh
HF_TOKEN=...
MODEL_PROFILE=qwen3_4b_inf2_4k
```

For the larger Inf2 profile, set `MODEL_PROFILE=qwen3_8b_inf2_32k`.

## Environment

The runtime exposes:

- `GET /health`
- `GET /v1/models`
- `POST /v1/chat/completions`

The runtime proxy listens on port `8080` and forwards `/v1/*` to the server on port `8000`.

## S3 Artifacts

```sh
S3_NEURON_ARTIFACTS_URI=s3://bucket/prefix/neuron-artifacts/qwen3_4b_inf2_4k/
SYNC_ARTIFACTS_BACK=true
```

## Notes

- Inf2 can be cost-effective, but it still adds compile and bootstrap overhead.
- Emberlane does not promise fixed wake times.
