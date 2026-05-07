# Release Checklist

Use this checklist before tagging an Emberlane alpha release.

## Local Validation

- [ ] `cargo fmt --check`
- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo run -- test local --runtime echo`
- [ ] `cargo run -- test local --runtime ollama`, if Ollama is installed and the configured model is pulled

## Terraform Validation

- [ ] `terraform fmt -check -recursive infra/terraform`, if Terraform is installed
- [ ] `terraform -chdir=infra/terraform init -backend=false`
- [ ] `terraform -chdir=infra/terraform validate`

Terraform validation must not require real AWS credentials.

## Optional AWS Live Validation

- [ ] Confirm AWS quota and cost expectations.
- [ ] Run the smallest live test, if credentials and opt-in are available.
- [ ] Verify `--destroy` completed successfully.
- [ ] Run `cargo run -- aws cleanup --dry-run` for the test environment.
- [ ] Record any remaining resources.

## Release Notes

- [ ] Update `CHANGELOG.md`.
- [ ] Confirm README known limitations are still accurate.
- [ ] Confirm no unfinished examples are presented as active features.
- [ ] Confirm no fixed wake-time, GPU availability, serverless GPU, or savings claims were added.

## Publish

- [ ] Tag the release.
- [ ] Create a GitHub release.
- [ ] Include known limitations.
- [ ] Link to setup, AWS deploy, live testing, and cleanup docs.
