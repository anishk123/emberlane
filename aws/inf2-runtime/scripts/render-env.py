#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path


def parse_scalar(value):
    value = value.strip()
    if value.startswith('"') and value.endswith('"'):
        return value[1:-1]
    if value.startswith("'") and value.endswith("'"):
        return value[1:-1]
    if value.isdigit():
        return int(value)
    if value.lower() == "true":
        return True
    if value.lower() == "false":
        return False
    return value


def load_models(path):
    models = {}
    current = None
    for raw in Path(path).read_text(encoding="utf-8").splitlines():
        if not raw.strip() or raw.strip().startswith("#") or raw.strip() == "models:":
            continue
        if raw.startswith("  ") and not raw.startswith("    ") and raw.strip().endswith(":"):
            current = raw.strip()[:-1]
            models[current] = {}
            continue
        if current and raw.startswith("    ") and ":" in raw:
            key, value = raw.strip().split(":", 1)
            models[current][key] = parse_scalar(value)
    return models


def profile_env(models, profile, model_id_override=None):
    if profile not in models:
        raise SystemExit(f"unknown MODEL_PROFILE '{profile}'. Available: {', '.join(sorted(models))}")
    model = dict(models[profile])
    if model_id_override:
        model["model_id"] = model_id_override
    return {
        "MODEL_PROFILE": profile,
        "MODEL_ID": model["model_id"],
        "DISPLAY_NAME": model.get("display_name", model["model_id"]),
        "RUNTIME": model.get("runtime", "vllm-neuron"),
        "INSTANCE_TYPE": model.get("instance_type", "inf2.xlarge"),
        "PORT": str(model.get("port", 8000)),
        "OPENAI_BASE_PATH": model.get("openai_base_path", "/v1"),
        "HEALTH_CHECK": model.get("health_check", "/health"),
        "TENSOR_PARALLEL_SIZE": str(model.get("tensor_parallel_size", 2)),
        "MAX_MODEL_LEN": str(model.get("max_model_len", 4096)),
        "MAX_NUM_SEQS": str(model.get("max_num_seqs", 32)),
        "BLOCK_SIZE": str(model.get("block_size", 8)),
        "NUM_GPU_BLOCKS_OVERRIDE": str(
            model.get("num_gpu_blocks_override", model.get("max_num_seqs", 32))
        ),
        "DEVICE": model.get("device", "neuron"),
        "STATUS": model.get("status", "hidden"),
    }


def shell_quote(value):
    return "'" + str(value).replace("'", "'\"'\"'") + "'"


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description="Render Emberlane Inf2 model profile env.")
    parser.add_argument("--models", default=str(root / "models.yaml"))
    parser.add_argument(
        "--profile",
        default=os.environ.get("MODEL_PROFILE", "qwen3_4b_inf2_4k"),
    )
    parser.add_argument("--model-id", default=os.environ.get("MODEL_ID"))
    parser.add_argument("--format", choices=["shell", "json"], default="shell")
    args = parser.parse_args()

    env = profile_env(load_models(args.models), args.profile, args.model_id)
    if args.format == "json":
        print(json.dumps(env, indent=2, sort_keys=True))
    else:
        for key, value in env.items():
            print(f"export {key}={shell_quote(value)}")


if __name__ == "__main__":
    main()
