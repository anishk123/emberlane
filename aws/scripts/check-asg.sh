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

echo "Auto Scaling Group: $ASG_NAME"
aws "${AWS_PROFILE_ARG[@]}" autoscaling describe-auto-scaling-groups \
  --auto-scaling-group-names "$ASG_NAME" \
  --region "$AWS_REGION" \
  --query 'AutoScalingGroups[0].{Desired:DesiredCapacity,Min:MinSize,Max:MaxSize,Instances:Instances[].{Id:InstanceId,Lifecycle:LifecycleState,Health:HealthStatus}}' \
  --output json

echo "Warm Pool:"
aws "${AWS_PROFILE_ARG[@]}" autoscaling describe-warm-pool \
  --auto-scaling-group-name "$ASG_NAME" \
  --region "$AWS_REGION" \
  --query '{Instances:Instances[].{Id:InstanceId,Lifecycle:LifecycleState},Pool:WarmPoolConfiguration}' \
  --output json || echo "warm pool unavailable or not permitted"
