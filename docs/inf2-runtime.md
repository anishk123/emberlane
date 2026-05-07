# Inf2 Runtime Pack

The Emberlane Inf2 Runtime Pack turns an AWS Inf2 EC2 instance into a wakeable OpenAI-compatible LLM runtime. It is designed to sit behind an ALB and be woken by Emberlane's `aws_asg` provider or Lambda WakeBridge.

## Why Inf2/Neuron

Inf2 instances provide AWS Inferentia accelerators. They can be cost-effective for some steady or warm-pooled inference workloads, but they are not universally cheaper than NVIDIA G instances and they add operational complexity: driver/runtime versions, model support, compilation, cache management, and longer first-boot paths.

Emberlane does not promise fixed wake times.

## First Success Path

The first documented target is:

- Profile: `llama32_1b`
- Model: `meta-llama/Llama-3.2-1B`
- Runtime: `vllm-neuron`
- Instance: `inf2.xlarge`
- Port: `8000`
- OpenAI base path: `/v1`
- Health check: `/health`

Qwen `qwen25_15b` uses `Qwen/Qwen2.5-1.5B-Instruct` and is marked experimental until validated on Inf2.

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
MODEL_PROFILE=llama32_1b
```

`scripts/download-model.sh` uses `huggingface_hub.snapshot_download`. You can also pre-bake weights into an AMI.

## Neuron Compiled Artifacts

Local cache:

```sh
NEURON_COMPILED_ARTIFACTS=/opt/emberlane/neuron-cache
```

Optional S3 sync:

```sh
S3_NEURON_ARTIFACTS_URI=s3://bucket/prefix/neuron-artifacts/llama32_1b/
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

## Emberlane Config

```toml
[[runtimes]]
id = "inf2-llama"
name = "Inf2 Llama Runtime"
provider = "aws_asg"
enabled = true
mode = "fast"
base_url = "http://your-alb-dns-name"
health_path = "/health"
startup_timeout_secs = 300
fast_wait_secs = 25
slow_retry_after_secs = 5
idle_ttl_secs = 300
max_concurrency = 1

[runtimes.config]
region = "us-west-2"
asg_name = "emberlane-inf2-llama-asg"
desired_capacity_on_wake = 1
desired_capacity_on_sleep = 0
warm_pool_expected = true
```

## Lambda WakeBridge

Use `aws/lambda-bridge` for buffered JSON requests. Use `aws/lambda-bridge-node` for response streaming where Lambda Function URL streaming is supported.

Lambda Function URLs do not support response streaming when the Lambda is configured inside a VPC. For private ALBs, plan for buffered responses or a different gateway pattern.

## Known Limitations

- No fixed wake-time promise.
- First boot may download weights and compile Neuron artifacts.
- Model support depends on Neuron, transformers, and vLLM versions.
- Qwen profile is experimental.
- Streaming support depends on the gateway and AWS networking shape.
