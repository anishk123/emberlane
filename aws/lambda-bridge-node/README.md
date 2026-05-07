# Emberlane Node Lambda WakeBridge

This is the optional streaming Lambda WakeBridge for Inf2/vLLM runtimes. It uses Lambda response streaming, wakes an Auto Scaling Group when health is not ready, and streams upstream `text/event-stream` chunks from `/v1/chat/completions`.

The Terraform deployment pack in `infra/terraform` uses this bridge by default. Run `aws/scripts/deploy-dev.sh` from the repo root so `npm install --omit=dev` runs before Terraform packages the Lambda zip.

`AWS_REGION` is provided by the Lambda runtime. Terraform also sets `EMBERLANE_AWS_REGION` so local tests and explicit deployments can choose the same region without trying to override a reserved Lambda variable.

## Behavior

- Requires `BASE_URL` to point at a stable ALB/NLB/API endpoint.
- Checks `BASE_URL + HEALTH_PATH`.
- Calls `autoscaling:SetDesiredCapacity` when unhealthy.
- In fast mode, waits up to `FAST_WAIT_SECS`.
- Returns HTTP 202 JSON if still warming.
- Streams SSE responses when the upstream response is `text/event-stream`.

## Important Limitation

Lambda Function URLs do not support response streaming when the Lambda function is configured inside a VPC. For private ALB deployments, use one of these paths:

- Use the Python buffered bridge without streaming.
- Expose the ALB publicly with strict auth/security groups where appropriate.
- Use an API Gateway/Lambda streaming setup that supports your networking needs.
- Use a different gateway pattern.

Do not assume fixed wake latency. Inf2 capacity, Warm Pool state, model download, and Neuron compilation all matter.
