# AWS Deploy From Zero

This guide deploys a dev/test Emberlane AWS path with Terraform:

```text
Client
  -> Lambda WakeBridge Function URL
  -> Auto Scaling Group desired capacity 0 -> 1
  -> EC2 GPU or Inf2 instance
  -> ALB target group
  -> Inf2 runtime /health
  -> OpenAI-compatible /v1/chat/completions
```

It does not use CDK and does not deploy unrelated cloud providers.

## Current Limitations

- This is a dev/test deployment pack, not a hardened production stack.
- The default path uses a public ALB and Lambda Function URL for simplicity.
- Lambda Function URL response streaming is not supported when Lambda is configured inside a VPC.
- First boot can include model download, container startup, and for Inf2, Neuron compilation; no fixed wake-time promise is made.
- You must provide a valid GPU, Neuron, or baked AMI ID.

## Cost Warning

GPU/Inf2 instances, ALB hours, EBS volumes, S3 storage, Lambda invocations, and data transfer can all cost money. Warm Pool stopped or hibernated instances can still incur EBS and related storage costs. Destroy the stack when you are done.

## Quota Warning

Most AWS accounts need quota increases before launching GPU or Inf2 instances. Request quota in the same region you plan to deploy, such as `us-west-2`.

## Prerequisites

- Terraform `>= 1.6.0`.
- AWS CLI and AWS credentials.
- GPU or Inf2 EC2 quota.
- A Hugging Face token and model access if your selected model requires it.
- `npm` for packaging the default Node WakeBridge Lambda dependencies.
- A selected AMI.

## AWS Credentials

Check credentials:

```sh
cargo run -- aws credentials check --profile emberlane
```

If credentials are missing, configure one of:

```sh
aws login --profile emberlane
cargo run -- aws credentials check --profile emberlane
cargo run -- aws init --profile emberlane --force
```

```sh
aws configure
```

```sh
aws configure --profile emberlane-dev
cargo run -- aws init --profile emberlane-dev
```

```sh
aws configure sso
aws sso login --profile <profile>
```

```sh
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-west-2
```

Emberlane stores profile and region only. Do not commit raw AWS secrets.

## Install Terraform

Emberlane's AWS deploy command renders Terraform variables and runs Terraform. Install Terraform before applying infrastructure.

On macOS with Homebrew:

```sh
brew tap hashicorp/tap
brew install hashicorp/tap/terraform
terraform version
```

If you only want to render the generated variables first, add `--plan-only`:

```sh
cargo run -- aws deploy --profile emberlane --model qwen35_9b --accelerator cuda --instance g5.2xlarge --mode balanced --plan-only
```

## Pick An AMI

Recommended first path:

- Use an AWS GPU Deep Learning AMI for CUDA/G5.
- Use an AWS Neuron Deep Learning AMI for Inf2/Neuron.
- Or use a baked AMI created after manually validating the runtime.

Emberlane auto-selects a sensible AMI for the chosen accelerator by default. CUDA/G5 uses the latest AWS Deep Learning Base AMI with Single CUDA (Ubuntu 22.04). Use `--ami-id` only if you want to pin a specific image or override the default.

## Configure Terraform

```sh
cd infra/terraform
cp terraform.tfvars.example terraform.tfvars
```

You usually only need to edit secrets or network settings. Emberlane will fill in the AMI for the selected accelerator:

```hcl
api_key = "long-random-dev-token"
```

The default public-ALB/non-VPC-Lambda path uses:

```hcl
allowed_ingress_cidr_blocks = ["0.0.0.0/0"]
```

That is dev-only. If you restrict this to your workstation IP, the Lambda WakeBridge may not be able to reach the ALB. For tighter ingress, use a private ALB plus a buffered/non-streaming Lambda plan, a controlled egress path, or another gateway pattern.

Optional Hugging Face token sources:

```hcl
hf_token_secret_arn = "arn:aws:secretsmanager:us-west-2:123456789012:secret:hf-token"
# or
hf_token_ssm_parameter_name = "/emberlane/hf-token"
```

## Terraform Init And Apply

From the repo root:

```sh
aws/scripts/deploy-dev.sh
```

Or manually:

```sh
cd infra/terraform
terraform init
terraform apply
```

The helper script installs Node Lambda dependencies before applying if `npm` is available.

## First Smoke Test

After apply:

```sh
cd infra/terraform
export LAMBDA_URL="$(terraform output -raw lambda_function_url)"
export API_KEY="long-random-dev-token"
../../aws/scripts/smoke-test-lambda.sh
```

The first request may return `202` while the ASG wakes. The smoke script retries until success or timeout.

## Expected First Boot Behavior

The first boot can include:

- EC2 launch and ALB target registration.
- Neuron driver/runtime startup.
- Runtime pack bootstrap if not using a baked AMI.
- Hugging Face model download.
- Neuron graph compilation.
- A proxy on port `8080` that serves `/health` and forwards `/v1/*` to the model server on port `8000`.
- Health check transition from `503` to `200`.
- For Qwen3.5 text-only serving on CUDA/G5, Emberlane passes a profile-specific `--max-model-len` and `--language-model-only` so the model fits the default `g5.2xlarge` path more reliably.

This can take several minutes. Warm Pools and baked AMIs reduce repeated work, but they do not guarantee a fixed wake time.

`startup_timeout_secs` is the wake-and-wait budget, not the ASG idle shutdown timer. If you are tuning responsiveness, adjust the startup timeout for slow model loads and the idle alarm separately for scale-to-zero behavior.

## Warm Pool Behavior

Terraform can create an ASG Warm Pool:

```hcl
enable_warm_pool = true
warm_pool_pool_state = "Stopped"
```

Warm Pools can keep prepared instances closer to ready state, but a depleted pool or model/runtime drift can still cause a cold path.

## Streaming Behavior

The Terraform pack deploys the Node WakeBridge with Lambda response streaming enabled:

```sh
../../aws/scripts/smoke-test-streaming.sh
```

Important caveat: Lambda Function URL response streaming is not available for Lambda functions configured inside a VPC. The default Terraform path keeps Lambda outside a VPC and uses a public ALB. If you need a private/internal ALB, use buffered responses, a different gateway pattern, or an AWS networking setup that explicitly supports streaming.

## S3 Artifact Bucket

The Terraform pack creates an encrypted S3 bucket by default. It is used for:

- Emberlane uploads when configured with S3 artifact storage.
- Optional Neuron compiled artifact sync.

The EC2 role gets read access to the artifact prefix. It only gets `s3:PutObject` for the prefix when `sync_artifacts_back = true`.

## Scale Down

```sh
cd infra/terraform
export ASG_NAME="$(terraform output -raw asg_name)"
../../aws/scripts/scale-down.sh
```

Terraform ignores ASG `desired_capacity` changes so Emberlane and Lambda can wake/sleep without Terraform fighting them.

## Destroy Stack

```sh
aws/scripts/destroy-dev.sh
```

Or:

```sh
cd infra/terraform
terraform destroy
```

## Troubleshooting

- `AccessDenied` on ASG: verify the Lambda role has `autoscaling:SetDesiredCapacity`, `autoscaling:DescribeAutoScalingGroups`, and `autoscaling:DescribeWarmPool`.
- Inf2 quota error: request quota for the selected instance type and region.
- AMI missing Neuron runtime: use a Neuron Deep Learning AMI or bake the validated runtime.
- Hugging Face access denied: verify model access and token source.
- ALB target unhealthy: check instance security group, target port `8080`, `/health`, and journald.
- Security group blocked: ALB should reach the instance on `runtime_port`; client CIDR should reach ALB port `80`.
- Lambda timeout: increase `lambda_timeout_secs` or use slow mode for immediate warming responses.
- Health never ready: SSH/SSM into the instance if enabled and test `curl localhost:8000/v1/models`.
- Model too large: start with `llama32_1b` on `inf2.xlarge`.
- Warm Pool empty: inspect `aws/scripts/check-asg.sh`; warm pools can be depleted or permission-limited.

## Production Hardening Checklist

- Add HTTPS listener and ACM certificate.
- Add WAF and stronger auth in front of public endpoints.
- Restrict `allowed_ingress_cidr_blocks`.
- Consider private ALB plus an alternate streaming plan.
- Add budget alarms.
- Review logging and redaction for prompts, API keys, and presigned URLs.
- Bake a validated AMI instead of bootstrap installing at launch.
- Tune health grace period and target group thresholds.
- Prefer SSM Session Manager over SSH.
