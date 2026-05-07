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
