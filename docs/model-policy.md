# Model Policy

Emberlane is opinionated on purpose. The public default should be easy to understand, easy to deploy, and hard to misuse.

## Why Llama Is Hidden

Llama profiles are still present for compatibility testing, but they are not part of the public default menu. The goal of Emberlane v1 is to make the AWS coding/research path simple, not to expose every possible model family at once.

## Why llama.cpp Is Not The AWS Cloud Runtime

The public AWS runtime is `vLLM CUDA`. That keeps the cloud path consistent and OpenAI-compatible. `llama.cpp` may be useful later, but it is not part of the public AWS path in this release.

## Why Inf2 Is Experimental

Inf2/Neuron is still experimental for the long-context profiles Emberlane cares about most. It is useful for evaluation and future optimization work, but the public menu should not imply that it is the safest or cheapest default. Emberlane does expose one lower-cost Inf2 experiment, `qwen3_8b_inf2_4k`, so you can benchmark a cheaper Neuron path without losing the G5/G6 options.

## Why Qwen3-8B-AWQ On G5 Is The Default

`Qwen/Qwen3-8B-AWQ` is a strong open model for coding, research, and agentic workflows. AWQ quantization reduces memory pressure, and the 32K context profile on `g5.2xlarge` is the practical first target for a single-GPU AWS deployment.

## How The Public Menu Is Grouped

| Profile | Best for | Kind |
| --- | --- | --- |
| `qwen3_8b_awq_32k_g5` | coding-simple, general research | text |
| `qwen3_8b_inf2_4k` | coding-simple, lower-cost Inf2 experiment | text |
| `qwen3_8b_awq_128k` | research-deep, long context | text |
| `gemma3_12b_128k` | research-general, image + text tasks | multimodal |
| `deepseek_r1_distill_qwen14b_64k` | coding-hard, reasoning | text |

## Why 32K Is The Initial Reliable Target

32K gives enough room for real coding and research tasks without pretending that every prompt will fit forever. It is a good balance between usefulness and deploy reliability, especially when the public default is the lower-cost G5 path.

## Why 128K Exists But Requires Validation

128K is useful for deep research, but it is not the default until Emberlane has a matching validation artifact. Profiles that need more memory or special rope scaling should be clearly labeled as advanced.

## How A Profile Becomes Validated

A profile should only move to `validated = true` after a matching validation artifact exists under `profiles/validation/<profile>/...`. Until then, the deploy path should require explicit acknowledgement.

## Why No Silent Fallback Is Allowed

Emberlane should never silently lower context length, change instance type, or switch pricing mode. If a deploy cannot satisfy the requested profile, the user should see the failure and the safe alternatives explicitly.
