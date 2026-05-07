# Emberlane

Your own OpenAI-compatible AI endpoint. Run locally with Ollama or deploy to AWS with scale-to-zero.

[![CI](https://github.com/anishk123/emberlane/actions/workflows/ci.yml/badge.svg)](https://github.com/anishk123/emberlane/actions)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)
![Terraform](https://img.shields.io/badge/terraform-validated-623CE4)
![AWS](https://img.shields.io/badge/AWS-supported-FF9900)
![Inferentia2](https://img.shields.io/badge/Inf2-experimental-blueviolet)

## What Emberlane Does

Emberlane is a local-first LLM gateway with one shipped CLI binary. It can:

- run local chat with the built-in echo runtime
- run local chat with Ollama
- upload text files and ask questions about one or more documents
- expose MCP tools for agent clients
- serve an HTTP and OpenAI-compatible API
- deploy an AWS scale-to-zero stack with Terraform

## Supported Interfaces

- CLI for local setup, AWS deploy, benchmarking, cost reports, diagnostics, and cleanup
- MCP stdio for agent/tool integration
- HTTP API for apps and internal services
- OpenAI-compatible chat endpoints for existing clients

## Local Quickstart

```sh
cargo run -- init
cargo run -- serve
cargo run -- chat echo "hello"
cargo run -- chat ollama "hello"
cargo run -- upload README.md
cargo run -- chat-file echo <file_id> "summarize this"
cargo run -- chat-files echo <file_id_1> <file_id_2> --message "compare these notes"
cargo run -- mcp
```

If Ollama is unavailable, Emberlane will tell you how to install it, start it, and pull the model it expects.

## AWS Quickstart

```sh
cargo run -- aws credentials check --profile your-profile
cargo run -- aws init --profile your-profile
cargo run -- aws models
cargo run -- aws modes
cargo run -- aws deploy --profile your-profile --mode balanced
cargo run -- aws chat "Explain scale-to-zero inference" --profile your-profile
cargo run -- aws benchmark --profile your-profile
cargo run -- aws cost-report --profile your-profile
cargo run -- aws destroy --profile your-profile
```

If you want a guided deploy path, Emberlane renders Terraform variables, applies the stack, and stores the resolved endpoint in `aws/emberlane.aws.toml`.

## File Storage And Multi-Document Chat

Emberlane stores uploaded files locally by default. For AWS deployments, you can switch to S3-backed storage so remote runtimes can fetch uploaded documents without depending on your laptop.

```sh
cargo run -- storage use local
cargo run -- storage use s3 --profile your-profile --region us-west-2
```

When S3 storage is enabled, Emberlane will create the derived artifact bucket on demand if your AWS credentials allow it.

Upload one or more text documents:

```sh
cargo run -- upload README.md docs/aws-deploy-from-zero.md
```

Then ask a question about one or more uploaded documents:

```sh
cargo run -- chat-files qwen35_9b <file_id_1> <file_id_2> --message "compare the AWS deployment notes"
```

For a single document, `chat-file` still works:

```sh
cargo run -- chat-file ollama <file_id> "summarize this"
```

## Model Choices

Use `cargo run -- aws models` to list the available model profiles.

The default AWS CUDA path is:

- `qwen35_9b`
- `g5.2xlarge`
- `balanced`

That is the recommended first path for public release. Inf2/Neuron is supported for experimental evaluation, but it is not presented as universally cheaper.

## Cost Modes

- `economy`: Spot instances, no warm pool, lowest idle cost
- `balanced`: On-demand instances, warm pool enabled, faster wake with some idle overhead
- `always-on`: On-demand instances, no warm pool, fastest steady-state response

## MCP Support

Emberlane exposes MCP tools for agents and developer tools. Supported tools:

- `emberlane_list_runtimes`
- `emberlane_status`
- `emberlane_chat`
- `emberlane_upload_file`
- `emberlane_chat_file`
- `emberlane_wake`
- `emberlane_sleep`

MCP is the recommended integration path for agent clients. The HTTP/OpenAI-compatible endpoint is the recommended path for app integration. The CLI is the recommended path for deployment, benchmarking, and operations.

## Architecture

Emberlane is intentionally simple. Local requests stay local; AWS requests go through Lambda WakeBridge and the ALB before they reach the ASG runtime.

```mermaid
graph TD
    Client([Client or Agent]) --> Interface[CLI / MCP / HTTP]

    subgraph LocalMachine["Local machine"]
        Interface --> Router[Emberlane router]
        Router --> LocalRuntime[Echo or Ollama runtime]
        Router --> LocalFiles[Local file storage]
    end

    subgraph AwsPath["AWS path"]
        Router --> Lambda[Lambda WakeBridge]
        Lambda --> ALB[Application Load Balancer]
        ALB --> ASG[EC2 Auto Scaling Group]
        ASG --> AwsRuntime[OpenAI-compatible runtime]
        Router --> S3[S3 artifact storage]
        AwsRuntime --> S3
    end
```

AWS is the first implemented hyperscaler backend. GCP and Azure are planned for later.

## Implemented Now

- local echo runtime
- local Ollama runtime
- file upload and chat with `.txt` / `.md`
- MCP stdio
- HTTP API
- OpenAI-compatible chat endpoint
- AWS Terraform deployment
- AWS benchmark and cost-report commands
- AWS S3-backed file storage

## Planned

- Python SDK
- TypeScript SDK
- GCP backend
- Azure backend
- richer UI

## Not Implemented Yet

- full RAG
- managed hosted service
- production multi-tenant auth
- dashboards

## License

Emberlane is dual-licensed under MIT OR Apache-2.0.
