# Cost Modes

Cost modes are first-class deploy choices:

```sh
cargo run -- aws modes
```

## economy

- ASG min `0`
- desired `0`
- max `1`
- Warm Pool disabled
- Spot instances
- Lowest idle cost expectation
- Coldest wake path
- Idle scale-down enabled

## balanced

- ASG min `0`
- desired `1`
- max `1`
- On-demand instances
- Warm Pool disabled by default
- Some storage/EBS cost while running
- Starts ready, then scales down after idle
- Idle scale-down enabled

## always-on

- ASG min `1`
- desired `1`
- max `1`
- Warm Pool disabled
- On-demand instances
- Highest idle cost
- Fastest response
- Idle scale-down disabled

Emberlane does not claim exact wake latency or cost savings. Use `emberlane aws benchmark` and real pricing inputs before making decisions.

`balanced` is the ready-on-deploy default; `economy` is the coldest path; `always-on` keeps one instance up and never auto-sleeps. Warm Pool is available as an advanced Terraform option, but it is not part of the default balanced semantics.
