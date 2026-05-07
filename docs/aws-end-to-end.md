# AWS End-To-End Deployment

This guide connects the pieces:

```text
Client
  -> Lambda WakeBridge or local Emberlane
  -> ASG desired capacity 0 -> 1
  -> EC2 GPU or Inf2 instance
  -> ALB target group
  -> /health
  -> /v1/chat/completions
```

## Steps

1. Choose a model profile with `cargo run -- aws models`.
2. Start with CUDA/G5 for the first path, or Inf2/Neuron for experimental cost-optimization work.
3. Bake an AMI or create a launch template that installs the runtime pack.
4. Create an ALB target group on port `8080` with health path `/health`.
5. Create an ASG with min `0`, desired `0`, max `1`.
6. Optionally add a Warm Pool in stopped or hibernated state.
7. Configure Emberlane with the `inf2-llama` `aws_asg` runtime.
8. Deploy Lambda WakeBridge.
9. Send an OpenAI-compatible request.
10. Verify streaming through the Node bridge if your Lambda networking supports it.

For a repeatable Terraform version of these resources, start with [AWS Deploy From Zero](aws-deploy-from-zero.md). The Terraform pack creates the dev/test VPC path, S3 artifact bucket, IAM roles, ALB, launch template, ASG, optional Warm Pool, and Lambda WakeBridge Function URL.

CUDA/G5 is the recommended first path for v1. Inf2/Neuron remains experimental; benchmark before claiming savings.

## AMI And Launch Template

Recommended first path:

- AWS Neuron Deep Learning AMI on Ubuntu.
- `inf2.xlarge`.
- Root EBS: at least 100 GB gp3.
- IAM role with optional S3 read/write for model/artifact buckets.
- CloudWatch logs optional.
- SSM optional.
- ECR pull permissions if using a private image.

User data should:

```sh
mkdir -p /opt/emberlane
# copy or clone the runtime pack to /opt/emberlane/inf2-runtime
cat >/etc/emberlane/inf2.env <<'ENV'
MODEL_PROFILE=llama32_1b
HF_HOME=/opt/emberlane/model-cache
TRANSFORMERS_CACHE=/opt/emberlane/model-cache
NEURON_COMPILED_ARTIFACTS=/opt/emberlane/neuron-cache
S3_NEURON_ARTIFACTS_URI=s3://bucket/prefix/neuron-artifacts/llama32_1b/
SYNC_ARTIFACTS_BACK=false
ENV
/opt/emberlane/inf2-runtime/bootstrap.sh
```

## Smoke Test

```sh
curl http://ALB_DNS_NAME/health
curl http://ALB_DNS_NAME/v1/models
curl -X POST http://ALB_DNS_NAME/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"meta-llama/Llama-3.2-1B","messages":[{"role":"user","content":"hello"}],"stream":false}'
```

Through Emberlane:

```sh
cargo run -- aws doctor inf2-llama
cargo run -- chat inf2-llama "hello"
```

Through Lambda WakeBridge:

```sh
curl -X POST "$WAKEBRIDGE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"meta-llama/Llama-3.2-1B","messages":[{"role":"user","content":"hello"}],"stream":false}'
```

Streaming, where supported:

```sh
curl -N -X POST "$NODE_WAKEBRIDGE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model":"meta-llama/Llama-3.2-1B","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

## Expected First Boot Behavior

First boot may include:

- Neuron runtime startup.
- Hugging Face model download.
- vLLM startup.
- Neuron graph compilation.
- ALB target health delay.

This may take several minutes. Warm Pools and baked AMIs reduce repeated startup work but do not guarantee fixed wake latency.

## Troubleshooting

- Quota error: request Inf2 quota in the target region.
- Model access denied: verify `HF_TOKEN` and model license access.
- Compile takes long: inspect `/var/log/emberlane/bootstrap.log` and journald.
- Health never ready: call `/v1/models` locally on the instance.
- ALB unhealthy: check target group port `8080`, security groups, and `/health`.
- Lambda VPC streaming limitation: Function URL response streaming is unavailable for VPC-configured Lambda.
- ASG set desired capacity denied: check `autoscaling:SetDesiredCapacity`.
- Model too large: start with `llama32_1b` on `inf2.xlarge`.
- Neuron device missing: verify instance type, AMI, drivers, and `/dev/neuron0`.
