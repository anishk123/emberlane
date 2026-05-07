data "aws_caller_identity" "current" {}

data "aws_availability_zones" "available" {
  state = "available"
}

locals {
  name_prefix = "${var.app_name}-${var.environment}"

  common_tags = {
    App              = var.app_name
    Project          = var.app_name
    Environment      = var.environment
    ManagedBy        = "emberlane"
    Component        = "emberlane-aws-deploy"
    EmberlaneTestRun = var.emberlane_test_run
  }

  normalized_artifact_prefix         = trimsuffix(trimprefix(var.artifact_prefix, "/"), "/")
  artifact_prefix                    = local.normalized_artifact_prefix == "" ? "" : "${local.normalized_artifact_prefix}/"
  normalized_neuron_artifacts_prefix = trimsuffix(trimprefix(var.neuron_artifacts_s3_prefix, "/"), "/")
  neuron_artifacts_prefix            = local.normalized_neuron_artifacts_prefix == "" ? "" : "${local.normalized_neuron_artifacts_prefix}/"

  generated_artifact_bucket_name = lower("${local.name_prefix}-${data.aws_caller_identity.current.account_id}-${var.aws_region}")
  artifact_bucket_name           = coalesce(var.artifact_bucket_name, local.generated_artifact_bucket_name)
  artifact_bucket_arn            = "arn:aws:s3:::${local.artifact_bucket_name}"
  artifact_objects_arn           = local.artifact_prefix == "" ? "arn:aws:s3:::${local.artifact_bucket_name}/*" : "arn:aws:s3:::${local.artifact_bucket_name}/${local.artifact_prefix}*"

  vpc_id            = var.create_vpc ? aws_vpc.this[0].id : var.vpc_id
  public_subnet_ids = var.create_vpc ? aws_subnet.public[*].id : var.public_subnet_ids

  alb_url = "http://${aws_lb.runtime.dns_name}"

  s3_neuron_artifacts_uri = var.enable_s3_artifact_sync ? "s3://${local.artifact_bucket_name}/${local.artifact_prefix}${local.neuron_artifacts_prefix}${var.model_profile}/" : ""
  lambda_source_dir       = "${path.module}/../../aws/lambda-bridge-node"
}
