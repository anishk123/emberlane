# Contributing

Thanks for helping build Emberlane. The project is intentionally scoped: local Ollama and echo runtimes, file-context chat, MCP stdio, and AWS-first deployment with Terraform.

## Local Setup

Install stable Rust, then run:

```sh
cargo run -- init
cargo run -- chat echo "hello"
```

Optional Ollama check:

```sh
ollama pull llama3.2:1b
cargo run -- chat ollama "hello"
```

## Before Opening A PR

Run the local checks:

```sh
cargo fmt -- --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Run the local harness when your change touches CLI, routing, files, MCP, or runtime behavior:

```sh
cargo run -- test local --runtime echo
```

If Terraform is installed and your change touches `infra/terraform`:

```sh
terraform fmt -check -recursive infra/terraform
terraform -chdir=infra/terraform init -backend=false
terraform -chdir=infra/terraform validate
```

Terraform validation must not require real AWS credentials in CI.

## AWS Tests

Live AWS tests create billable resources. Do not run them casually and do not make them part of normal CI.

Live tests require explicit opt-in:

```sh
cargo run -- test aws \
  --model tiny_demo \
  --accelerator cuda \
  --instance g4dn.xlarge \
  --mode economy \
  --destroy \
  --yes-i-understand-this-creates-aws-resources
```

Use `--destroy` unless you are debugging. If you use `--keep-on-failure`, include cleanup status in the PR.

## PR Expectations

- Keep the active product surface focused on chat, chat-with-file, local Ollama/echo, MCP, and AWS Terraform deployment.
- Do not add new cloud providers until the AWS path is solid.
- Do not commit AWS credentials, API keys, Terraform state with secrets, presigned URLs, or model provider tokens.
- Update docs and tests for behavior changes.
- Avoid claims about fixed wake times, GPU availability, serverless GPU, or guaranteed savings.
- Prefer small, readable changes over broad abstraction.
