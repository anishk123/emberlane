# Model Profiles

Model profiles live in `profiles/models.toml` and are listed with:

```sh
cargo run -- aws models
```

Status values:

- `recommended`: good first AWS/CUDA target.
- `cheap_demo`: lower-cost demo target.
- `experimental`: available but not validated as a default recommendation.
- `inf2_experimental`: Inf2/Neuron path; benchmark before relying on it.

Profiles map a friendly name to model ID, default accelerator, recommended instance, runtime, status, and max model length.
Some profiles also include fallback instance hints for capacity-constrained regions.

Emberlane does not claim a model is validated unless the repo includes a real validation record.

Some profile keys end with `_economy` for historical reasons. Those are not AWS cost modes; they are tighter-memory Inf2 model profiles.

Current default CUDA first path:

- `qwen35_9b` on `g5.2xlarge`
- Model ID: `Qwen/Qwen3.5-9B`
- Runtime: `vllm-cuda`
- Text-only serving on CUDA uses the profile's max context length, `--language-model-only`, and `--reasoning-parser qwen3` so the default `g5.2xlarge` path stays practical and follows the official vLLM recipe. The default context is `1024`.

Experimental quantized CUDA path:

- `qwen35_9b_quantized` on `g5.2xlarge`
- Model ID: `inference-optimization/Qwen3.5-9B-quantized.w4a16`
- Runtime: `vllm-cuda`
- This profile points Emberlane at a documented 4-bit vLLM-ready checkpoint and starts with a larger `2048` context window. It is the profile to try if you want more headroom on the same 24 GB GPU, but it is still experimental and should be benchmarked before becoming a default.

Current experimental Inf2 Qwen3 path:

- `qwen3_4b_inf2` on `inf2.xlarge`
- Model ID: `Qwen/Qwen3-4B-Instruct-2507`
- Runtime: `vllm-neuron`
- This is the first conservative Qwen3/Inf2 profile in Emberlane. Treat it as experimental and benchmark before relying on it.
