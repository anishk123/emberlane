data "archive_file" "wakebridge_node" {
  type        = "zip"
  source_dir  = local.lambda_source_dir
  output_path = "${path.module}/lambda-wakebridge-node.zip"
}

resource "aws_cloudwatch_log_group" "wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  name              = "/aws/lambda/${local.name_prefix}-wakebridge"
  retention_in_days = var.log_retention_days
  tags              = local.common_tags
}

resource "aws_lambda_function" "wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  function_name    = "${local.name_prefix}-wakebridge"
  role             = aws_iam_role.lambda_wakebridge[0].arn
  runtime          = var.lambda_runtime
  handler          = "index.handler"
  filename         = data.archive_file.wakebridge_node.output_path
  source_code_hash = data.archive_file.wakebridge_node.output_base64sha256
  timeout          = var.lambda_timeout_secs
  memory_size      = var.lambda_memory_mb

  environment {
    variables = merge({
      EMBERLANE_AWS_REGION      = var.aws_region
      ASG_NAME                  = aws_autoscaling_group.runtime.name
      BASE_URL                  = local.alb_url
      HEALTH_PATH               = var.health_path
      MODE                      = lower(var.wake_mode)
      FAST_WAIT_SECS            = tostring(var.fast_wait_secs)
      STARTUP_TIMEOUT_SECS      = tostring(var.startup_timeout_secs)
      RETRY_AFTER_SECS          = tostring(var.retry_after_secs)
      DESIRED_CAPACITY_ON_WAKE  = tostring(var.desired_capacity_on_wake)
      DESIRED_CAPACITY_ON_SLEEP = tostring(var.desired_capacity_on_sleep)
      }, merge(
      var.require_alb_secret ? { ALB_SECRET = random_password.alb_secret[0].result } : {},
      var.api_key == null ? {} : {
        API_KEY = var.api_key
    }))
  }

  depends_on = [
    aws_cloudwatch_log_group.wakebridge,
    aws_iam_role_policy_attachment.lambda_wakebridge
  ]

  tags = local.common_tags
}

resource "aws_lambda_function_url" "wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  function_name      = aws_lambda_function.wakebridge[0].function_name
  authorization_type = var.function_url_auth_type
  invoke_mode        = "RESPONSE_STREAM"

  cors {
    allow_credentials = false
    allow_headers     = ["authorization", "content-type"]
    allow_methods     = ["GET", "POST"]
    allow_origins     = ["*"]
    max_age           = 300
  }
}
