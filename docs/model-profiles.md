# Model Profiles

Model profiles live in `profiles/models.toml` and are listed with:

```sh
cargo run -- aws models
```

## Status Values

- `recommended`: public first pick.
- `advanced`: public, but not the first thing to try.
- `hidden`: compatibility or lab profile not shown in the normal picker.

## Public Task Groups

- `Single agent`
- `Simple coding`
- `Simple agent`
- `Hard coding`
- `Hard agent`
- `Reasoning`
- `General research`
- `Deep research`
- `Multimodal`

## Public Defaults

- `qwen35_2b` on `g5.2xlarge` using `Qwen/Qwen3.5-2B`
- `qwen35_2b_awq` on `g5.2xlarge` using `cyankiwi/Qwen3.5-2B-AWQ-4bit`
- `qwen35_9b` on `g6e.2xlarge` using `Qwen/Qwen3.5-9B`
- `qwen35_9b_awq` on `g6e.2xlarge` using `QuantTrio/Qwen3.5-9B-AWQ`
- `qwen25_15b_inf2_economy` on `inf2.xlarge`
- Model ID: `Qwen/Qwen2.5-1.5B-Instruct`
- Runtime: `vllm-neuron`

## Other Public Profiles

- `qwen3_8b_inf2_32k` on `inf2.8xlarge`, with `inf2.24xlarge` as the larger-memory fallback
- `qwen3_8b_awq_32k_g5` on `g5.2xlarge`
- `qwen3_8b_awq_32k` on `g6e.xlarge`
- `qwen3_8b_awq_128k` on `g6e.2xlarge`
- `gemma3_12b_128k` on `g6e.2xlarge`
- `deepseek_r1_distill_qwen14b_64k` on `g6e.2xlarge`

## Hidden Compatibility Profiles

- Legacy Qwen2.5 Inf2 profiles remain hidden compatibility options.
- `qwen35_9b_quantized` remains hidden
- `qwen3_4b_inf2` on `inf2.xlarge`
- `llama31_8b` on `g5.2xlarge`
- `llama32_1b_inf2` on `inf2.8xlarge`

These profiles stay out of the normal picker unless you pass `--experimental` or `--show-hidden`.
