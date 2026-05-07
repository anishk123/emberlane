# AWS WakeBridge

AWS WakeBridge adds a real AWS wake-routing path to Emberlane. An `aws_asg` runtime scales an EC2 Auto Scaling Group up when a request arrives, waits for a stable runtime endpoint to pass health checks, proxies the original request, and can scale the ASG back down on sleep.

## Architecture

```text
Client or agent
  -> Emberlane local server or Lambda WakeBridge
  -> aws autoscaling set-desired-capacity
  -> Auto Scaling Group, optionally with Warm Pool
  -> ALB/NLB/stable base_url
  -> runtime GET /health and POST /chat or /v1/chat/completions
```

Emberlane v0.2 requires a stable `base_url`, usually an ALB, NLB, or API endpoint in front of the ASG. It does not discover dynamic EC2 private IPs.

## When To Use ASG vs Warm Pool

Use a plain ASG when occasional cold starts are acceptable and you want the simplest AWS setup. Add a Warm Pool when startup work is expensive and you want instances pre-initialized in a stopped or hibernated state. Warm Pools can still be depleted, misconfigured, or slower than expected, so Emberlane does not make fixed wake-time claims.

## Required AWS Resources

- Launch template or launch configuration for the runtime instance.
- Auto Scaling Group with min size commonly `0`, desired capacity `0`, and max size at least `1`.
- Optional Warm Pool.
- ALB, NLB, or another stable endpoint for `base_url`.
- Security groups allowing Emberlane/Lambda/ALB to reach the runtime health and chat ports.
- IAM permissions for Auto Scaling actions.

## Emberlane Runtime Config

```toml
[[runtimes]]
id = "aws-echo"
name = "AWS Echo Runtime"
provider = "aws_asg"
enabled = true
mode = "fast"
base_url = "http://your-alb-dns-name"
health_path = "/health"
startup_timeout_secs = 600
fast_wait_secs = 25
slow_retry_after_secs = 5
idle_ttl_secs = 300
max_concurrency = 2

[runtimes.config]
region = "us-west-2"
asg_name = "emberlane-echo-asg"
desired_capacity_on_wake = 1
desired_capacity_on_sleep = 0
aws_cli = "aws"
profile = ""
warm_pool_expected = true
```

Run checks locally:

```sh
cargo run -- aws doctor aws-echo
cargo run -- aws status aws-echo
cargo run -- aws render-iam aws-echo
```

## Request Behavior

In fast mode, Emberlane checks `base_url + health_path`. If unhealthy, it calls `aws autoscaling set-desired-capacity --desired-capacity <desired_capacity_on_wake>`, waits up to `fast_wait_secs`, and proxies the original request if health becomes ready. If the runtime is still warming, Emberlane returns HTTP 202.

In slow mode, Emberlane triggers the same ASG wake and returns the warming response quickly.

Sleep calls `set-desired-capacity` with `desired_capacity_on_sleep`, usually `0`. Emberlane does not terminate individual EC2 instances manually.

Important distinction: `startup_timeout_secs` is for the wake-and-wait path, not for idle shutdown. Idle scale-down is controlled by the ASG request-count alarm.

## Runtime Contract

The runtime behind the stable endpoint should expose:

- `GET /health`: return 200 only when the model/runtime is ready.
- `POST /chat`: Emberlane chat request shape.
- Or `POST /v1/chat/completions`: OpenAI-compatible non-streaming chat.

For file-aware runtime calls, pair `aws_asg` with the optional S3 artifact store. Emberlane can route a body containing `s3_uri` and an optional `presigned_url` so the remote runtime does not depend on local disk paths. See `docs/s3-artifact-store.md`.

## Lambda WakeBridge

The deployable example lives in `aws/lambda-bridge`. It receives Function URL or API Gateway v2 requests, checks health, scales the ASG, and proxies JSON requests to `BASE_URL + path`. When `API_KEY` is set, the bridge accepts either `Authorization: Bearer <API_KEY>` or `x-api-key: <API_KEY>`.

If `BASE_URL` is a public ALB, Lambda does not need VPC networking. If `BASE_URL` is an internal ALB or private IP, Lambda must be configured with the correct VPC subnets and security groups.

The Python Lambda bridge buffers responses. For streaming OpenAI-compatible responses, use `aws/lambda-bridge-node` where Lambda response streaming is supported. Lambda Function URLs do not support response streaming for Lambda functions configured inside a VPC.

## Minimal IAM Policy

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "autoscaling:SetDesiredCapacity",
        "autoscaling:DescribeAutoScalingGroups",
        "autoscaling:DescribeWarmPool"
      ],
      "Resource": [
        "arn:aws:autoscaling:REGION:ACCOUNT_ID:autoScalingGroup:*:autoScalingGroupName/ASG_NAME",
        "*"
      ]
    }
  ]
}
```

Replace `REGION`, `ACCOUNT_ID`, and `ASG_NAME`. Keep `SetDesiredCapacity` scoped tightly. Some describe actions may require `*`.

## Cost Model

Running instances cost while active. Stopped or hibernated Warm Pool instances mainly incur storage and related attached-resource costs. ALB, NAT, public IPv4, storage, logs, and data transfer can still cost money even when desired capacity is 0.

## Limitations

- No fixed wake-time promise.
- Warm Pool depletion can fall back to a cold launch path.
- Python Lambda WakeBridge v0.2 does not stream.
- A stable `base_url` is required.
- AWS CLI credentials or IAM are required.
- Emberlane does not discover dynamic EC2 private IPs in v0.2.

## Troubleshooting

- `AccessDenied`: run `emberlane aws render-iam <runtime_id>` and compare the policy attached to your user, role, or Lambda.
- ASG not found: verify `region`, `asg_name`, and optional `profile`.
- Health never ready: check ALB target health, instance logs, startup scripts, and the runtime `/health` contract.
- ALB target unhealthy: confirm security groups allow ALB to reach the instance runtime port.
- Lambda cannot reach runtime: public ALB works without VPC; private endpoints need Lambda VPC networking.
- Model server not started: check systemd, cloud-init, and application logs on the EC2 instance.
- Warm pool empty: describe the warm pool and confirm prepared capacity and lifecycle state.
