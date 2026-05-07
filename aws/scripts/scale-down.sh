#!/usr/bin/env bash
set -euo pipefail

ASG_NAME="${1:-${ASG_NAME:-}}"
AWS_REGION="${AWS_REGION:-us-west-2}"
AWS_PROFILE_ARG=()

if [[ -n "${AWS_PROFILE:-}" ]]; then
  AWS_PROFILE_ARG=(--profile "$AWS_PROFILE")
fi

if [[ -z "$ASG_NAME" ]]; then
  echo "usage: ASG_NAME=emberlane-dev-inf2-asg $0 [asg_name]" >&2
  exit 1
fi

aws "${AWS_PROFILE_ARG[@]}" autoscaling set-desired-capacity \
  --auto-scaling-group-name "$ASG_NAME" \
  --desired-capacity "${DESIRED_CAPACITY_ON_SLEEP:-0}" \
  --region "$AWS_REGION" \
  --no-honor-cooldown

echo "requested scale down for $ASG_NAME"
