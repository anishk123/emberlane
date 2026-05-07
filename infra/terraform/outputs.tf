output "artifact_bucket_name" {
  description = "S3 artifact bucket name."
  value       = local.artifact_bucket_name
}

output "artifact_prefix" {
  description = "S3 artifact prefix."
  value       = local.artifact_prefix
}

output "alb_dns_name" {
  description = "Runtime ALB DNS name."
  value       = aws_lb.runtime.dns_name
}

output "alb_url" {
  description = "Runtime ALB URL used as Emberlane base_url."
  value       = local.alb_url
}

output "target_group_arn" {
  description = "Runtime target group ARN."
  value       = aws_lb_target_group.runtime.arn
}

output "asg_name" {
  description = "Inf2 runtime Auto Scaling Group name."
  value       = aws_autoscaling_group.runtime.name
}

output "launch_template_id" {
  description = "Inf2 launch template ID."
  value       = aws_launch_template.runtime.id
}

output "instance_security_group_id" {
  description = "Inf2 instance security group ID."
  value       = aws_security_group.instance.id
}

output "alb_security_group_id" {
  description = "ALB security group ID."
  value       = aws_security_group.alb.id
}

output "lambda_function_url" {
  description = "Lambda WakeBridge Function URL."
  value       = var.deploy_lambda_wakebridge ? aws_lambda_function_url.wakebridge[0].function_url : null
}

output "emberlane_runtime_config" {
  description = "Example Emberlane aws_asg runtime config using this stack."
  value       = <<-TOML
  [[runtimes]]
  id = "inf2-llama"
  name = "Inf2 Llama Runtime"
  provider = "aws_asg"
  enabled = true
  mode = "fast"
  base_url = "${local.alb_url}"
  health_path = "${var.health_path}"
  startup_timeout_secs = ${var.startup_timeout_secs}
  fast_wait_secs = ${var.fast_wait_secs}
  slow_retry_after_secs = ${var.retry_after_secs}
  idle_ttl_secs = 300
  max_concurrency = 1

  [runtimes.config]
  region = "${var.aws_region}"
  asg_name = "${aws_autoscaling_group.runtime.name}"
  desired_capacity_on_wake = ${var.desired_capacity_on_wake}
  desired_capacity_on_sleep = ${var.desired_capacity_on_sleep}
  warm_pool_expected = ${var.enable_warm_pool}
  TOML
}
