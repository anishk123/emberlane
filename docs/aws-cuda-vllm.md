# AWS CUDA / vLLM

The public AWS runtime is `vLLM CUDA`.

`balanced` is the ready-first operating point: the stack comes up ready, then scales down after idle. `economy` is the coldest scale-to-zero path.

The default cloud profile is `qwen3_8b_awq_32k_g5` on `g5.2xlarge`.

Use:

```sh
cargo run -- aws deploy --model qwen3_8b_awq_32k_g5 --accelerator cuda --instance g5.2xlarge --mode economy --acknowledge-unvalidated
```

The Terraform pack renders:

- `accelerator = "cuda"`
- `runtime_pack = "cuda-vllm"`
- a CUDA model profile from `profiles/models.toml`
- a scale mode from `cargo run -- aws modes`

You must choose a real GPU AMI. A practical first path is an AWS Deep Learning AMI with NVIDIA drivers, Docker, and NVIDIA container runtime available, or a baked AMI that already starts vLLM.

The dev bootstrap path starts `vllm/vllm-openai:latest` through Docker when Docker/GPU runtime are present. For production, bake and validate the AMI.

Emberlane keeps the Hugging Face cache on the instance disk, forces vLLM safetensors prefetch, and uses a 32K default `max_model_len` on the Qwen3 AWQ profile so the default CUDA path stays practical on the single-GPU `g5.2xlarge` path.

If you want more context headroom, try the experimental `qwen3_8b_awq_128k` profile. It points at the same model family with rope scaling, but it is still an opt-in experiment rather than the default.

No fixed latency or savings claims are made. Benchmark your model, AMI, region, and prompt mix.
