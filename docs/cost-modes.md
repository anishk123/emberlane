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
- Warm Pool enabled
- On-demand instances
- Some storage/EBS/prepared-capacity cost
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

AWS does not allow a Warm Pool on an ASG that requests Spot instances, so `balanced` uses on-demand instances to keep the warm-pool path valid. `balanced` is the ready-on-deploy default; `economy` is the coldest path; `always-on` keeps one instance up and never auto-sleeps.
