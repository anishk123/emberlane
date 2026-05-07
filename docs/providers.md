# Emberlane Providers

Supported in the default build:

- `mock`: deterministic tests.
- `static_http`: health check and JSON proxy to an already running HTTP runtime.
- `local_process`: starts a configured local command, polls health, and proxies JSON.
- `ollama`: starts or connects to local Ollama and translates chat requests to `/api/chat`.
- `aws_asg`: scales an EC2 Auto Scaling Group with the AWS CLI, waits for a stable `base_url` health endpoint, and proxies JSON.

Unsupported:

- Docker
- Broad cloud-provider abstractions beyond `aws_asg`
- Plugin providers
- Hosted runtime platforms

See `docs/aws-wakebridge.md` for the AWS ASG provider.
