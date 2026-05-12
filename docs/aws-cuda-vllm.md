# AWS CUDA / vLLM

The public NVIDIA path is `vLLM CUDA`.

`balanced` is the ready-first operating point: the stack comes up ready, then scales down after idle. `economy` is the coldest scale-to-zero path.

Use the CUDA path when you want NVIDIA headroom or to compare against the Inf2 defaults:

- `qwen35_2b` on `g5.2xlarge` for the cheapest new-model simple agent / simple coding lane, using `cyankiwi/Qwen3.5-2B-AWQ-4bit`
- `qwen35_9b` on `g6e.2xlarge` for hard coding / deep research / reasoning, using `QuantTrio/Qwen3.5-9B-AWQ`
- `qwen3_8b_awq_32k_g5` on `g5.2xlarge` for simple coding if you want a text-only AWQ path
- `qwen3_8b_awq_32k` on `g6e.xlarge` for simple agents
- `qwen3_8b_awq_128k` on `g6e.2xlarge` for deep research
- `deepseek_r1_distill_qwen14b_64k` on `g6e.2xlarge` for hard agentic work

Use:

```sh
cargo run -- aws deploy --model qwen35_2b --accelerator cuda --instance g5.2xlarge --mode economy
```

The Terraform pack renders:

- `accelerator = "cuda"`
- `runtime_pack = "cuda-vllm"`
- a CUDA model profile from `profiles/models.toml`
- a scale mode from `cargo run -- aws modes`

You must choose a real GPU AMI. A practical first path is an AWS Deep Learning AMI with NVIDIA drivers, Docker, and NVIDIA container runtime available, or a baked AMI that already starts vLLM.

The dev bootstrap path starts `vllm/vllm-openai:latest` through Docker when Docker/GPU runtime are present. For production, bake and validate the AMI.

Emberlane keeps the Hugging Face cache on the instance disk, forces vLLM safetensors prefetch, and uses conservative context caps on the public CUDA profiles so the default path stays practical on the single-GPU `g5.2xlarge` and the smaller G6e lanes.

If you want more context headroom, try the `qwen3_8b_awq_128k` profile. It points at the same model family with rope scaling and stays in the public menu for deeper research. If you want the newer Qwen family instead, start with `qwen35_2b` and step up to `qwen35_9b`.

No fixed latency or savings claims are made. Benchmark your model, AMI, region, and prompt mix.
