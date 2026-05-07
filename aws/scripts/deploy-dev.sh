#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LAMBDA_DIR="$ROOT_DIR/aws/lambda-bridge-node"
TF_DIR="$ROOT_DIR/infra/terraform"

if ! command -v terraform >/dev/null 2>&1; then
  echo "terraform is required. Install Terraform >= 1.6.0 first." >&2
  exit 1
fi

if [[ -f "$LAMBDA_DIR/package.json" ]]; then
  if command -v npm >/dev/null 2>&1; then
    echo "[emberlane] installing Lambda WakeBridge Node dependencies"
    (cd "$LAMBDA_DIR" && npm install --omit=dev)
  else
    echo "[emberlane] warning: npm not found; Terraform will package the Node bridge without node_modules." >&2
    echo "[emberlane] install npm or run npm install --omit=dev in $LAMBDA_DIR before apply." >&2
  fi
fi

cd "$TF_DIR"
terraform init
terraform apply "$@"
