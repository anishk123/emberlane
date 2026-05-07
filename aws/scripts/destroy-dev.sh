#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TF_DIR="$ROOT_DIR/infra/terraform"

if ! command -v terraform >/dev/null 2>&1; then
  echo "terraform is required. Install Terraform >= 1.6.0 first." >&2
  exit 1
fi

cd "$TF_DIR"
terraform destroy "$@"
