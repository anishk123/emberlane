## Summary


## Tests Run

- [ ] `cargo fmt -- --check`
- [ ] `cargo test`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo run -- test local --runtime echo`
- [ ] Terraform checks, if applicable
- [ ] AWS live tests, only if explicitly run

## Documentation

- [ ] README/docs updated, or not needed
- [ ] CHANGELOG updated, or not needed
- [ ] No unfinished examples promoted as active features

## AWS And Secrets

- [ ] No AWS credentials, API keys, tokens, Terraform state secrets, or presigned URLs committed
- [ ] Real AWS tests were not run, or they are described below
- [ ] Cleanup was verified if AWS resources were created

## AWS Live Test Notes

If real AWS resources were created, include:

- Command run:
- Region/profile:
- Resources destroyed? yes / no
- Remaining resources, if any:
