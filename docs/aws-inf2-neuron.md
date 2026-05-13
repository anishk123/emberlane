# AWS Inf2 / Neuron

Inf2/Neuron is a first-class Emberlane path for the cheapest public AWS coding and research setups.

Neuron still adds real operational complexity:

- model compatibility
- Neuron runtime versions
- graph compilation
- cache management
- longer first-boot paths

The public Inf2 menu is centered on the conservative Qwen2.5 economy lane:

- `qwen25_15b_inf2_economy` on `inf2.xlarge` using `Qwen/Qwen2.5-1.5B-Instruct`
- `qwen3_8b_inf2_32k` on `inf2.8xlarge`, with `inf2.24xlarge` as the larger-memory fallback

Legacy Qwen3 Inf2 experiments remain hidden and only appear with `--experimental` or `--show-hidden`.

Neuron's vLLM guide also recommends downloading Qwen-family checkpoints locally instead of serving the Hugging Face ID directly when shard-on-load is involved, and setting a matching `num_gpu_blocks_override`. Emberlane's runtime pack does that for the Inf2 profiles it exposes.

`inf2.8xlarge` is a cheaper Qwen3-8B validation lane, not a larger accelerator-memory lane. AWS lists both `inf2.xlarge` and `inf2.8xlarge` with one Inferentia2 chip and 32 GB accelerator memory. Use it before jumping to `inf2.24xlarge`, but expect `inf2.24xlarge` to be the first real Inf2 step-up when the failure is accelerator memory rather than host RAM or startup comfort.

Use:

```sh
cargo run -- aws deploy --model qwen25_15b_inf2_economy --accelerator inf2 --instance inf2.xlarge --mode economy
```

Benchmark before claiming savings. Warm Pools and cached artifacts can help, but do not guarantee fixed wake times.
