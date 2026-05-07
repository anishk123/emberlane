# Terraform Pack Validation

The Terraform pack is intentionally validated locally without AWS credentials.

Normal project tests check for:

- Required Terraform files.
- Required variables.
- ASG `desired_capacity` lifecycle ignore rules.
- Lambda Function URL wiring.
- Autoscaling IAM actions.
- User-data Inf2 environment rendering hooks.
- Executable smoke/cleanup scripts.

Optional manual validation:

```sh
cd infra/terraform
terraform fmt -check
terraform init -backend=false
terraform validate
```

`terraform validate` checks syntax and provider schemas, but a real `terraform plan` needs AWS credentials and a real `ami_id`.
