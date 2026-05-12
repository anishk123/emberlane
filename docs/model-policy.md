# Model Policy

Emberlane is opinionated on purpose. The public default should be easy to understand, easy to deploy, and hard to misuse.

## Public Defaults

| Profile | Best for | Kind |
| --- | --- | --- |
| `qwen35_2b` | single agent, simple coding | multimodal/text |
| `qwen35_9b` | hard coding, hard agent, reasoning, deep research | multimodal/text |
| `qwen3_4b_inf2_4k` | simple coding, simple agents | text |
| `qwen3_8b_awq_32k_g5` | simple coding | text |
| `qwen3_8b_awq_32k` | simple agents | text |
| `qwen3_8b_inf2_32k` | Inf2 32K validation, deep research | text |
| `qwen3_8b_awq_128k` | deep research | text |
| `gemma3_12b_128k` | multimodal | multimodal |
| `deepseek_r1_distill_qwen14b_64k` | hard agent | text |

## Why Qwen3.5 Is The First Public CUDA Path

Qwen3.5 is the newest public Qwen family Emberlane exposes. The public CUDA profiles use the AWQ community repos `cyankiwi/Qwen3.5-2B-AWQ-4bit` and `QuantTrio/Qwen3.5-9B-AWQ`, which keep the newest Qwen behavior available in a practical single-GPU shape.

That makes Qwen3.5 on CUDA a real, documented path, not a guess.

## Why The Menu Keeps Cheapest First

The public menu is sorted by a cheapest-first rule inside each family. For Qwen3.5 that means `qwen35_2b` appears before `qwen35_9b`. For Qwen3, the Inf2 and CUDA starters stay around as lower-risk follow-ups.

`g5.2xlarge` is the cheapest public place to try Qwen3.5-2B, while `g6e.2xlarge` is the cleaner place to try Qwen3.5-9B. If Qwen3.5-9B needs more room, `g6e.4xlarge` is the next safe step-up.

## Why Qwen3.5-2B Is The First Newer Starter

`Qwen/Qwen3.5-2B` remains the base model family behind the cheapest public CUDA lane, but Emberlane serves the AWQ repo `cyankiwi/Qwen3.5-2B-AWQ-4bit` to keep the `g5.2xlarge` path practical. It is still the obvious stepping stone before users move up to the larger 9B profile.

## Why Qwen3.5-9B Exists

`Qwen/Qwen3.5-9B` is the newer model for hard coding, hard agent, reasoning, and deep research tasks. Emberlane serves the AWQ repo `QuantTrio/Qwen3.5-9B-AWQ` text-only so the runtime stays practical, but the underlying model is still multimodal-capable.

## Why 32K Still Matters

32K gives enough room for real coding and research tasks without pretending that every prompt will fit forever. It is a good balance between usefulness and deploy reliability.

## Why 128K Exists

128K is useful for deep research and long prompts. It stays in the public menu because some users need the extra headroom, but the simple coding path does not depend on it.

Legacy Qwen2.5 Inf2 compatibility profiles remain hidden from the public menu and only show up with `--experimental` or `--show-hidden`.

## Why Llama Is Hidden

Llama profiles are still present for compatibility testing, but they are not part of the public default menu. The goal of Emberlane v1 is to make the AWS coding and research path simple, not to expose every possible model family at once.

## Why llama.cpp Is Not The AWS Cloud Runtime

The public AWS runtime is `vLLM CUDA` and `vLLM Neuron`. That keeps the cloud path consistent and OpenAI-compatible. `llama.cpp` may be useful later, but it is not part of the public AWS path in this release.

## Why No Silent Fallback Is Allowed

Emberlane should never silently lower context length, change instance type, or switch pricing mode. If a deploy cannot satisfy the requested profile, the user should see the failure and the safe alternatives explicitly.

## How A Profile Becomes Ready

A profile becomes ready for the public menu when its deploy path is wired, its defaults are stable, and Emberlane can render the exact launch command and instance shape without silently changing it.

## Why The Task Buckets Exist

The task buckets are there to answer the questions users actually ask:

- simple coding
- simple agents
- hard coding
- hard agents
- general research
- deep research
- multimodal

That keeps the picker useful without forcing people to decode raw model names first.
