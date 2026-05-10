# AWS Inf2 / Neuron

Inf2/Neuron is experimental in Emberlane.

It can be worth benchmarking for specific workloads, but it is not a blanket replacement for NVIDIA G instances and Neuron adds real operational complexity:

- model compatibility
- Neuron runtime versions
- graph compilation
- cache management
- longer first-boot paths

The runtime pack lives in `aws/inf2-runtime`. The model profiles ending in `_inf2` are marked `inf2_experimental` or `optional` when they are exposed as a lower-cost experiment.

Use:

```sh
cargo run -- aws deploy --model llama32_1b_inf2 --accelerator inf2 --instance inf2.xlarge --mode balanced
```

The first Qwen3 Inf2 profile is `qwen3_4b_inf2` on `inf2.xlarge` with `Qwen/Qwen3-4B-Instruct-2507`. It is experimental, not a blanket recommendation.

The first Qwen3-8B Inf2 experiment is `qwen3_8b_inf2_4k` on `inf2.xlarge` with `Qwen/Qwen3-8B`, a local checkpoint path, `max_model_len = 4096`, `max_num_seqs = 8`, `block_size = 32`, and `num_gpu_blocks_override = 8`.

Benchmark before claiming savings. Warm Pools and cached artifacts can help, but do not guarantee fixed wake times.
