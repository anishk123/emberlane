# Emberlane AWS Terraform Deployment

This Terraform pack deploys a dev/test AWS path for a wakeable OSS LLM runtime. CUDA/G5 is the recommended first path; Inf2/Neuron is available as experimental.

```text
Client
  -> Lambda WakeBridge Function URL
  -> Auto Scaling Group
  -> EC2 GPU or Inf2 instance
  -> ALB target group
  -> /health
  -> /v1/chat/completions
```

It is intentionally Terraform, not CDK, and focuses only on the AWS deployment pieces needed by Emberlane.

## What It Creates

- Optional minimal public VPC with two public subnets.
- S3 artifact bucket with encryption, public access block, and versioning.
- IAM roles for the EC2 runtime and Lambda WakeBridge.
- ALB, target group, and HTTP listener.
- Launch template with CUDA/vLLM or Inf2/Neuron runtime user-data.
- Auto Scaling Group with desired capacity set by Emberlane mode:
  - `economy`: `0`
  - `balanced`: `1`
  - `always-on`: `1`
- Optional ASG Warm Pool.
- Lambda Function URL running the Node WakeBridge for response streaming where supported.
- Optional ALB header gate for extra friction in dev/test; disabled by default.

No NAT Gateway is created by default to avoid surprise cost.

## Prerequisites

- Terraform `>= 1.6.0`.
- AWS CLI credentials with permissions to create the listed resources.
- GPU or Inf2 quota in the selected region.
- A valid GPU Deep Learning AMI, Neuron Deep Learning AMI, or baked Emberlane runtime AMI.
- `npm` if deploying the default Node WakeBridge package.
- Hugging Face access/token if your selected model requires it.

## Quick Dev Deploy

```sh
cp terraform.tfvars.example terraform.tfvars
# edit terraform.tfvars and set allowed_ingress_cidr_blocks, api_key
../../aws/scripts/deploy-dev.sh
```

Emberlane auto-selects a sensible AMI for the chosen accelerator when you run `cargo run -- aws deploy`. Use `--ami-id` only to override the default or pin a specific image. The generated `terraform.tfvars.json` already includes the selected image.

## Important Defaults

- The ALB is public by default for a simple dev/test Lambda Function URL path.
- `allowed_ingress_cidr_blocks = ["0.0.0.0/0"]` is dev-only. A non-VPC Lambda Function URL does not have a stable customer-controlled source IP, so tightening ALB ingress requires a different networking/gateway plan.
- Lambda Function URL auth defaults to `NONE`; set `api_key`.
- The ALB header gate is optional. Emberlane can run without `X-Emberlane-Secret` by default to keep the public dev path easy to use.
- ASG `desired_capacity` is ignored by Terraform lifecycle so Emberlane/Lambda can wake and sleep the group without Terraform fighting it.
- Warm Pool is enabled by default, but it does not guarantee fixed wake latency.

## Validation

```sh
terraform fmt -check
terraform init -backend=false
terraform validate
```

`terraform validate` does not deploy AWS resources. A real plan/apply needs credentials and a real AMI.

## Smoke Tests

After apply:

```sh
export LAMBDA_URL="$(terraform output -raw lambda_function_url)"
export API_KEY="your-api-key"
../../aws/scripts/smoke-test-lambda.sh
../../aws/scripts/smoke-test-streaming.sh
```

Scale down:

```sh
export ASG_NAME="$(terraform output -raw asg_name)"
../../aws/scripts/scale-down.sh
```

Destroy:

```sh
../../aws/scripts/destroy-dev.sh
```

For the full from-zero walkthrough, see [docs/aws-deploy-from-zero.md](../../docs/aws-deploy-from-zero.md).
