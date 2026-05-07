# AWS CUDA / vLLM

CUDA/G5 is the recommended first AWS path for Emberlane.

`balanced` is the default operating point: the stack comes up ready, then scales down after idle. `economy` is the coldest scale-to-zero path.

For Qwen3.5, Emberlane copies the official text-only serving shape: `Qwen/Qwen3.5-9B`, `--language-model-only`, and `--reasoning-parser qwen3`.

Use:

```sh
cargo run -- aws deploy --model qwen35_9b --accelerator cuda --instance g5.2xlarge --mode balanced
```

The Terraform pack renders:

- `accelerator = "cuda"`
- `runtime_pack = "cuda-vllm"`
- a CUDA model profile from `profiles/models.toml`
- a scale mode from `cargo run -- aws modes`

You must choose a real GPU AMI. A practical first path is an AWS Deep Learning AMI with NVIDIA drivers, Docker, and NVIDIA container runtime available, or a baked AMI that already starts vLLM.

The dev bootstrap path starts `vllm/vllm-openai:latest` through Docker when Docker/GPU runtime are present. For production, bake and validate the AMI.

No fixed latency or savings claims are made. Benchmark your model, AMI, region, and prompt mix.
