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

Emberlane does not claim a model is validated unless the repo includes a real validation record.

Current default CUDA first path:

- `qwen35_9b` on `g5.2xlarge`
- Model ID: `Qwen/Qwen3.5-9B`
- Runtime: `vllm-cuda`
- Text-only serving on CUDA uses the profile's max context length, `--language-model-only`, and `--reasoning-parser qwen3` so the default `g5.2xlarge` path stays practical and follows the official vLLM recipe.
