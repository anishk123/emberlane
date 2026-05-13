# Inf2 Runtime Pack

The Emberlane Inf2 Runtime Pack turns an AWS Inf2 EC2 instance into a wakeable OpenAI-compatible LLM runtime. It is designed to sit behind an ALB and be woken by Emberlane's `aws_asg` provider or Lambda WakeBridge.

## Public Inf2 Targets

The public Inf2 targets are centered on the conservative Qwen2.5 economy lane:

- `Qwen/Qwen2.5-1.5B-Instruct` on `inf2.xlarge`
- `Qwen/Qwen3-8B` on `inf2.8xlarge`, with `inf2.24xlarge` as the larger-memory fallback

Legacy Qwen3 Inf2 experiments remain hidden and only appear with `--experimental` or `--show-hidden`.

Neuron guidance matters here: the vLLM Neuron docs recommend serving Qwen-family checkpoints from a local path instead of the Hugging Face model ID when shard-on-load is involved, and the compiled profile needs a matching `num_gpu_blocks_override`. Emberlane's runtime pack and Inf2 profiles already wire that in.

## Why Inf2

Inf2 can be cost-effective for steady or warm-pooled inference workloads, but it still adds operational complexity:

- model compatibility
- Neuron runtime versions
- graph compilation
- cache management
- longer first-boot paths

Emberlane does not promise fixed wake times.

## Build Image

```sh
cd aws/inf2-runtime
docker build -f Dockerfile.neuron -t emberlane-inf2-neuron .
```

The Dockerfile uses a configurable AWS Neuron PyTorch inference base image and builds/installs vLLM with Neuron support. This can be slow. For real deployments, bake the validated runtime into an AMI or image after a manual Inf2 test.

## Manual Instance First

Start with one instance before using ASG:

1. Pick an AWS Neuron Deep Learning AMI for Ubuntu.
2. Use `inf2.xlarge`.
3. Attach at least 100 GB gp3.
4. Ensure `/dev/neuron0` exists.
5. Copy `aws/inf2-runtime` to `/opt/emberlane/inf2-runtime`.
6. Create `/etc/emberlane/inf2.env`.
7. Run `sudo /opt/emberlane/inf2-runtime/bootstrap.sh`.
8. Run `/opt/emberlane/inf2-runtime/scripts/smoke-test.sh --wait`.

## Model Weights

For gated Hugging Face models, set:

```sh
HF_TOKEN=...
MODEL_PROFILE=qwen25_15b_inf2_economy
```

For the larger Inf2 profile, set `MODEL_PROFILE=qwen3_8b_inf2_32k`.

`scripts/download-model.sh` uses `huggingface_hub.snapshot_download`. You can also pre-bake weights into an AMI.

## Neuron Compiled Artifacts

Local cache:

```sh
NEURON_COMPILED_ARTIFACTS=/opt/emberlane/neuron-cache
```

Optional S3 sync:

```sh
S3_NEURON_ARTIFACTS_URI=s3://bucket/prefix/neuron-artifacts/qwen25_15b_inf2_economy/
SYNC_ARTIFACTS_BACK=true
```

On boot the runtime syncs from S3 before startup. If enabled, it syncs back after the server exits. Keep the local EBS cache for faster repeated starts.

## Health And OpenAI Contract

The runtime exposes:

- `GET /health`
- `GET /v1/models`
- `POST /v1/chat/completions`
- `POST /v1/completions` if supported by the vLLM build

`/health` returns 503 until `/v1/models` is reachable. The runtime proxy listens on port `8080` and forwards `/v1/*` to the vLLM server on port `8000`.

## ALB And ASG

ALB target group:

- Port `8080`
- Health path `/health`
- Success matcher `200`
- Give health checks enough grace for model startup.

ASG:

- min `0`
- desired `0`
- max `1` initially
- Warm Pool optional, stopped or hibernated

Launch template/user data should install the runtime pack, write `/etc/emberlane/inf2.env`, and run `bootstrap.sh`.

For a complete Terraform deployment that creates the ALB, launch template, ASG, Warm Pool, S3 artifact bucket, and Lambda WakeBridge, see [AWS Deploy From Zero](aws-deploy-from-zero.md).
