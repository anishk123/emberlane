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
- Lowest idle cost expectation
- Coldest wake path

## balanced

- ASG min `0`
- desired `0`
- max `1`
- Warm Pool enabled
- Some storage/EBS/prepared-capacity cost
- Warmer starts when the pool has capacity

## always-on

- ASG min `1`
- desired `1`
- max `1`
- Warm Pool disabled
- Highest idle cost
- Fastest response

Emberlane does not claim exact wake latency or cost savings. Use `emberlane aws benchmark` and real pricing inputs before making decisions.
