# Emberlane

AWS scale-to-zero LLM inference, with Ollama for development.

Run a single binary. Deploy model profiles to AWS when you want the cloud to wake up only on demand, or use Ollama locally when you are iterating.

[![CI](https://github.com/anishk123/emberlane/actions/workflows/ci.yml/badge.svg)](https://github.com/anishk123/emberlane/actions)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)
![Rust](https://img.shields.io/badge/rust-1.75%2B-orange)
![Terraform](https://img.shields.io/badge/terraform-validated-623CE4)
![AWS](https://img.shields.io/badge/AWS-supported-FF9900)
![Inferentia2](https://img.shields.io/badge/Inf2-supported-blueviolet)

## At A Glance

| | |
| --- | --- |
| ☁️ **AWS scale-to-zero** | Wake a model only when requests arrive, then let it sleep again when idle. |
| 🦙 **Ollama for dev** | Keep local iteration fast and simple with the runtime people already know. |
| 📦 **Model profiles** | Choose a profile once, then override model, mode, or instance when needed. |
| 🔌 **CLI / MCP / HTTP** | Deploy, automate, and integrate through the interface that fits the job. |

Emberlane is for people who want their own OpenAI-compatible endpoint on AWS, with local Ollama as the friendly dev path.

## What Emberlane Does

Emberlane ships as one CLI binary and can:

- ☁️ deploy an AWS scale-to-zero stack with Terraform
- 💬 run local chat with the built-in echo runtime
- 🦙 run local chat with Ollama
- 📄 upload text files and ask questions about one or more documents
- 🧠 expose MCP tools for agent clients
- 🌐 serve an HTTP and OpenAI-compatible API

## How Defaults Work

Emberlane is designed to be useful by default and adjustable when you need it.

- `profiles/models.toml` defines the model profiles Emberlane knows about.
- `emberlane.toml` stores local defaults for the CLI, local storage, and runtimes.
- `aws/emberlane.aws.toml` stores AWS deploy defaults such as region, profile, model, mode, and endpoint.
- CLI flags override config when you want a one-off change.

Recommended AWS first path:

- runtime: `vLLM CUDA`
- model: `qwen35_2b`
- repo: `Qwen/Qwen3.5-2B`
- instance: `g5.2xlarge`
- task: `Single agent / simple coding`
- mode: `economy` on Spot, or `balanced` when you want ready-first behavior

If you want extra memory headroom on the same lane:

- model: `qwen35_2b_awq`
- repo: `cyankiwi/Qwen3.5-2B-AWQ-4bit`
- instance: `g5.2xlarge`
- task: `Single agent / simple coding`

If you want a stronger Qwen3.5 step-up:

- runtime: `vLLM CUDA`
- model: `qwen35_9b`
- repo: `Qwen/Qwen3.5-9B`
- instance: `g6e.2xlarge`
- task: `Hard coding / deep research / reasoning`
- mode: `balanced` when you want ready-first behavior

If you want the AWQ variant of the stronger lane:

- model: `qwen35_9b_awq`
- repo: `QuantTrio/Qwen3.5-9B-AWQ`
- instance: `g6e.2xlarge`
- task: `Hard coding / deep research / reasoning`

If you want the cheaper Inf2 lane:

- runtime: `vLLM Neuron`
- model: `qwen3_4b_inf2_4k`
- repo: `Qwen/Qwen3-4B`
- instance: `inf2.xlarge`
- task: `Simple coding`
- mode: `economy` on Spot, or `balanced` when you want ready-first behavior

If you want more room on Inf2:

- runtime: `vLLM Neuron`
- model: `qwen3_8b_inf2_32k`
- repo: `Qwen/Qwen3-8B`
- instance: `inf2.8xlarge`
- safe fallback: `inf2.24xlarge`
- task: `Deep research`
- mode: `balanced` when you want ready-first behavior

When you run `aws deploy` interactively, Emberlane now asks for the model on the instance first, then asks for cost mode next. The cost-mode prompt defaults to `economy / Spot`.

Use `cargo run -- aws models` to inspect profiles, `cargo run -- aws modes` to inspect cost modes, and `cargo run -- aws print-config` to inspect the current AWS defaults before you deploy.

If AWS says an instance type is unavailable or temporarily exhausted, run `cargo run -- aws doctor` first. Emberlane will now report the region check and suggest nearby fallback sizes from the profile instead of silently switching hardware.

If you want to compare multiple models, deploy one profile at a time and use `aws benchmark` and `aws cost-report` to compare the real tradeoffs.

## Supported Interfaces

- 🖥️ CLI for local setup, AWS deploy, benchmarking, cost reports, diagnostics, and cleanup
- 🤖 MCP stdio for agent/tool integration
- 🌐 HTTP API for apps and internal services
- 🧩 OpenAI-compatible chat endpoints for existing clients

For Aider setup, see [docs/aider.md](docs/aider.md). For the public model policy, see [docs/model-policy.md](docs/model-policy.md).

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
cargo run -- aws prices show
cargo run -- aws print-config
cargo run -- aws deploy --profile your-profile --mode balanced
cargo run -- aws validate-profile qwen35_2b --aws-profile your-profile --auto-approve
cargo run -- aws chat "Explain scale-to-zero inference" --profile your-profile
cargo run -- aws benchmark --profile your-profile
cargo run -- aws cost-report --profile your-profile
cargo run -- aws destroy --profile your-profile
```

If you want a guided deploy path, Emberlane renders Terraform variables, applies the stack, and stores the resolved endpoint in `aws/emberlane.aws.toml`.

To refresh or inspect cached AWS pricing estimates:

```sh
cargo run -- aws prices refresh --region us-west-2
cargo run -- aws prices show --region us-west-2
cargo run -- aws models --refresh-prices
cargo run -- aws models --offline
```

## Multi-Model Workflow

Emberlane is designed so you can keep the defaults simple and still compare several models over time.

- Start with one default model profile.
- Deploy another profile when you want to compare behavior or cost.
- Keep inactive models scaled down or destroyed so you only pay for what is actually up.
- Use `aws benchmark` and `aws cost-report` to make the tradeoffs visible instead of guessing.

Example:

```sh
cargo run -- aws deploy --profile your-profile --model qwen3_4b_inf2_4k --mode balanced
cargo run -- aws deploy --profile your-profile --model deepseek_r1_distill_qwen14b_64k --mode economy
cargo run -- aws benchmark --profile your-profile
```

## AWS Terraform Deployment

For repeatable AWS setup, see [docs/aws-deploy-from-zero.md](docs/aws-deploy-from-zero.md). The CLI renders Terraform variables, runs plan/apply, and manages destroy for you.

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
cargo run -- chat-files qwen3_4b_inf2_4k <file_id_1> <file_id_2> --message "compare the AWS deployment notes"
```

For a single document, `chat-file` still works:

```sh
cargo run -- chat-file ollama <file_id> "summarize this"
```

## Model Choices

Use `cargo run -- aws models` to list the available model profiles.

Each profile describes one model and the hardware Emberlane recommends for it.

The public AWS runtimes are `vLLM CUDA` for the default public path and `vLLM Neuron` for Inf2 paths.

The default AWS path is `qwen35_2b` on `g5.2xlarge` in `economy` mode. That is the recommended first path for public release.

That default uses the official base Qwen3.5 2B repo and text-only serving so the single-GPU CUDA path stays practical. If you want the AWQ sibling for more headroom, choose `qwen35_2b_awq`.

`economy` is Spot + ready-first, `balanced` is On-Demand + ready-first, and `always-on` is On-Demand + never sleeps.

Model selection guide:

| Profile | Best for | Kind | Notes |
| --- | --- | --- | --- |
| `qwen35_2b` | single agent, simple coding | 32K | multimodal/text | `Qwen/Qwen3.5-2B` on `g5.2xlarge`; Emberlane serves text-only |
| `qwen35_2b_awq` | single agent, simple coding | 32K | multimodal/text | `cyankiwi/Qwen3.5-2B-AWQ-4bit` on `g5.2xlarge`; Emberlane serves text-only |
| `qwen35_9b` | hard coding, hard agent, reasoning, deep research | 32K | multimodal/text | `Qwen/Qwen3.5-9B` on `g6e.2xlarge`; Emberlane serves text-only |
| `qwen35_9b_awq` | hard coding, hard agent, reasoning, deep research | 32K | multimodal/text | `QuantTrio/Qwen3.5-9B-AWQ` on `g6e.2xlarge`; Emberlane serves text-only |
| `qwen3_4b_inf2_4k` | simple coding, simple agent | 4K | text | cheapest public Inf2 starter |
| `qwen3_8b_inf2_32k` | deep research, large context | 32K | text | cheaper Inf2 lane on `inf2.8xlarge`; safe fallback is `inf2.24xlarge` |
| `qwen3_8b_awq_32k_g5` | simple coding | 32K | text | budget CUDA path |
| `qwen3_8b_awq_32k` | simple agent, coding, research | 32K | text | larger CUDA path |
| `qwen3_8b_awq_128k` | deep research, complex agent | 128K | text | deepest CUDA context option |
| `gemma3_12b_128k` | research, multimodal | 128K | multimodal | use this when you want vision input |
| `deepseek_r1_distill_qwen14b_64k` | reasoning, hard agent | 64K | text | slower, more deliberate |

Legacy Qwen2.5 Inf2 compatibility profiles remain hidden unless you pass `--experimental` or `--show-hidden`.

Inf2 is now a first-class AWS option. Use the Qwen3 Inf2 profiles when you want the cheapest public path, and keep the CUDA profiles for NVIDIA headroom or larger-context comparisons.

For multi-model comparison:

- pick one model profile
- deploy it
- benchmark it
- destroy it if you are done
- repeat with another profile

## Cost Modes

| Mode | Default capacity | Warm pool | Pricing | Good for |
| --- | --- | --- | --- | --- |
| `economy` | min `0`, desired `1`, max `1` | Disabled | Spot | Ready on deploy, scales down after idle |
| `balanced` | min `0`, desired `1`, max `1` | Disabled by default | On-demand | Ready on deploy, scales down after idle |
| `always-on` | min `1`, desired `1`, max `1` | Disabled | On-demand | Never auto-sleeps |

These are defaults, not hard limits. You can override them in config or on the command line when you need something specific.

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

- managed hosted service
- production multi-tenant auth
- dashboards

## License

Emberlane is dual-licensed under MIT OR Apache-2.0.
