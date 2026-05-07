# Emberlane Lambda WakeBridge

This is a small Python Lambda bridge for AWS ASG-backed Emberlane runtimes. It is useful when you want an always-available HTTPS entrypoint that can scale an Auto Scaling Group from 0 to 1, wait briefly for a stable runtime endpoint to become healthy, and proxy the original JSON request.

## Contract

- `GET /healthz` returns `{"ok": true}` without touching the ASG.
- Runtime requests are proxied to `BASE_URL + path`.
- File metadata in request bodies, including `s3_uri` and `presigned_url`, is passed through unchanged.
- If `API_KEY` is set, requests require `Authorization: Bearer <API_KEY>`.
- `MODE=fast` wakes the ASG and waits up to `FAST_WAIT_SECS`.
- `MODE=slow` wakes the ASG and immediately returns a 202 warming response.
- Requests with `"stream": true` return a clear error because this Python bridge buffers responses. Use `aws/lambda-bridge-node` for Lambda response streaming where AWS networking constraints allow it.

## Environment

See `env.example`. The important values are:

- `ASG_NAME`: Auto Scaling Group to scale.
- `BASE_URL`: stable ALB/NLB/API URL in front of the runtime.
- `HEALTH_PATH`: health endpoint, usually `/health`.
- `DESIRED_CAPACITY_ON_WAKE`: usually `1`.
- `DESIRED_CAPACITY_ON_SLEEP`: usually `0`.

## Deploy With SAM

```sh
sam build
sam deploy --guided
```

For development, a public ALB and Lambda Function URL can be the fastest path. If `BASE_URL` is an internal ALB or private IP, configure Lambda VPC subnets and security groups yourself.

For a repeatable Terraform dev/test deployment that wires Lambda WakeBridge to an ALB, ASG, optional Warm Pool, S3 artifact bucket, and Inf2 launch template, see `infra/terraform` and `docs/aws-deploy-from-zero.md`.

The Terraform pack prefers `aws/lambda-bridge-node` when response streaming is needed. This Python bridge remains the simplest buffered JSON bridge.

## IAM

The function needs:

- `autoscaling:SetDesiredCapacity`
- `autoscaling:DescribeAutoScalingGroups`
- `autoscaling:DescribeWarmPool`

Scope `SetDesiredCapacity` to your ASG ARN where practical. Some describe calls may need broader resource scope depending on AWS IAM evaluation.

## Limitations

- Streaming is not supported in the Python Lambda WakeBridge v0.2.
- Wake time depends on AMI, warm pool state, model startup, health checks, and AWS capacity.
- The bridge assumes `BASE_URL` is stable; it does not discover per-instance IPs.
