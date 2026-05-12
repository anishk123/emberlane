# Live AWS Testing

Emberlane includes a real integration-test harness for local and AWS deployments.

Live AWS tests create billable resources. They require explicit opt-in:

```sh
--yes-i-understand-this-creates-aws-resources
```

or:

```sh
export EMBERLANE_ALLOW_AWS_TESTS=1
```

## Credentials

Check credentials first:

```sh
cargo run -- aws credentials check --profile emberlane
```

Supported credential paths:

```sh
aws login --profile emberlane
cargo run -- aws init --profile emberlane --force
```

```sh
aws configure
```

```sh
aws configure --profile emberlane-dev
cargo run -- aws init --profile emberlane-dev
```

```sh
aws configure sso
aws sso login --profile <profile>
```

```sh
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-west-2
```

Do not commit secrets. Emberlane stores profile and region, not raw AWS access keys.

## Local Tests

```sh
cargo run -- test local --runtime echo
cargo run -- test local --runtime ollama
```

Reports are written to `.emberlane/test-runs/local/<timestamp>/`.

## Smallest AWS Test

```sh
cargo run -- test aws \
  --model tiny_demo \
  --accelerator cuda \
  --instance g4dn.xlarge \
  --mode economy \
  --destroy \
  --yes-i-understand-this-creates-aws-resources
```

Use `--keep-on-failure` only for debugging. Otherwise diagnostics are collected and cleanup is attempted when `--destroy` is set.

## Multiple Models

```sh
cargo run -- test aws \
  --models qwen3_4b_inf2_4k,qwen3_8b_inf2_32k \
  --accelerator inf2 \
  --instance inf2.xlarge \
  --mode economy \
  --destroy \
  --yes-i-understand-this-creates-aws-resources
```

The default AWS path now starts from `qwen3_4b_inf2_4k` on `inf2.xlarge`.

## Matrix

```sh
cargo run -- test aws-matrix \
  --config tests/aws-matrix.example.toml \
  --destroy \
  --yes-i-understand-this-creates-aws-resources
```

Run one case:

```sh
cargo run -- test aws-matrix --config tests/aws-matrix.example.toml --only tiny-demo-cuda-economy --destroy --yes-i-understand-this-creates-aws-resources
```

## Reports

AWS reports are written to:

```text
.emberlane/test-runs/aws/<timestamp>/
```

Each case contains:

- `report.json`
- `report.md`
- `terraform.log`
- `diagnostics.json`

The summary contains success/failure, model, accelerator, mode, benchmark output, cost-report output, diagnostics, and cleanup status.

## Cleanup

```sh
cargo run -- aws cleanup --environment emberlane-it-example --dry-run
cargo run -- aws cleanup --test-run .emberlane/test-runs/aws/<timestamp>/<model> --force
```

Cleanup is conservative. It only uses known Terraform directories or tagged Emberlane resources.

## Failure Workflow

1. Inspect `summary.md`.
2. Inspect the case `diagnostics.json`.
3. If resources remain, run `cargo run -- aws cleanup --dry-run` first.
4. Use `--keep-on-failure` only when you need resources left running for debugging.

## Quotas And Cost

AWS tests may need GPU/Inf2 quota. ALB, EC2, EBS, S3, Lambda, logs, and data transfer can all cost money. Use `--destroy` for routine tests.
