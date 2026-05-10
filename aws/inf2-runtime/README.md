# Emberlane Inf2 Runtime Pack

This pack runs an OpenAI-compatible vLLM/Neuron server on AWS Inf2 so Emberlane AWS WakeBridge has a real runtime to wake.

The first success path is `llama32_1b`:

- Model: `meta-llama/Llama-3.2-1B`
- Runtime: `vllm-neuron`
- Instance: `inf2.xlarge`
- vLLM port: `8000`
- ALB/nginx port: `8080`
- Health: `/health`
- OpenAI-compatible API: `/v1/chat/completions`

`qwen25_15b` is included as experimental until validated on Inf2.
`qwen3_4b` is the first conservative Qwen3 Inf2 starting point and uses `Qwen/Qwen3-4B-Instruct-2507` on `inf2.xlarge`.
`qwen3_8b_inf2_4k` is the first Qwen3-8B Inf2 experiment. It uses a local checkpoint path, `max_model_len=4096`, `max_num_seqs=8`, `block_size=32`, and `num_gpu_blocks_override=8` on `inf2.xlarge`.

## Files

- `models.yaml`: model profiles.
- `scripts/render-env.py`: renders profile env vars without PyYAML.
- `start-server.sh`: starts `vllm serve` with Neuron flags, then launches a small proxy on port `8080`.
- `bootstrap.sh`: prepares directories, validates `/dev/neuron0`, installs systemd, and starts the service.
- `server/health_proxy.py`: serves `/health` and proxies `/v1/*` to the upstream model server.
- `nginx/nginx.conf`: optional reference config if you prefer nginx instead of the built-in Python proxy.
- `Dockerfile.neuron`: Neuron/vLLM image scaffold.

## Build Image

```sh
docker build -f Dockerfile.neuron -t emberlane-inf2-neuron .
```

The vLLM Neuron build can be slow and version-sensitive. For practical AWS use, validate on one Inf2 instance, then bake an AMI or image with model/cache artifacts.

## Manual Inf2 Smoke Path

1. Launch an `inf2.xlarge` using an AWS Neuron Deep Learning AMI.
2. Attach at least 100 GB gp3 root volume.
3. Copy this runtime pack to `/opt/emberlane/inf2-runtime`.
4. Create `/etc/emberlane/inf2.env`:

```sh
MODEL_PROFILE=llama32_1b
HF_TOKEN=...
HF_HOME=/opt/emberlane/model-cache
TRANSFORMERS_CACHE=/opt/emberlane/model-cache
NEURON_COMPILED_ARTIFACTS=/opt/emberlane/neuron-cache
```

For Qwen3 Inf2 experiments, use `MODEL_PROFILE=qwen3_4b`.
For the Qwen3-8B Inf2 experiment, use `MODEL_PROFILE=qwen3_8b_inf2_4k`.

5. Run:

```sh
sudo ./bootstrap.sh
./scripts/smoke-test.sh --wait
```

## Compiled Artifact Cache

Use EBS for the local cache:

```sh
NEURON_COMPILED_ARTIFACTS=/opt/emberlane/neuron-cache
```

Optionally sync artifacts with S3:

```sh
S3_NEURON_ARTIFACTS_URI=s3://bucket/prefix/neuron-artifacts/llama32_1b/
SYNC_ARTIFACTS_BACK=true
```

On boot, `start-server.sh` syncs from S3 before starting vLLM. If `SYNC_ARTIFACTS_BACK=true`, it syncs the cache back after the server exits.

## Health Contract

`GET /health` returns 200 only when `GET /v1/models` succeeds. Before the model is ready it returns 503 so ALB and Emberlane do not route too early. The same proxy on port `8080` forwards `/v1/*` to vLLM on port `8000`.

## ASG/ALB

Use an ALB target group with:

- Port: `8080`
- Health path: `/health`
- Matcher: `200`

Initial ASG shape:

- min `0`
- desired `0`
- max `1`
- optional Warm Pool in stopped or hibernated state

No fixed wake-time claims are made. First boot may include model download and Neuron compilation.
