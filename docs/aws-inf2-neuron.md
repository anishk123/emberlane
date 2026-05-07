# AWS Inf2 / Neuron

Inf2/Neuron is experimental in Emberlane.

It may be promising for cost optimization, but Neuron adds real operational complexity:

- model compatibility
- Neuron runtime versions
- graph compilation
- cache management
- longer first-boot paths

The runtime pack lives in `aws/inf2-runtime`. The model profiles ending in `_inf2` are marked `inf2_experimental`.

Use:

```sh
cargo run -- aws deploy --model llama32_1b_inf2 --accelerator inf2 --instance inf2.xlarge --mode balanced
```

Benchmark before claiming savings. Warm Pools and cached artifacts can help, but do not guarantee fixed wake times.
