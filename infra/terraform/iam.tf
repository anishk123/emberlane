data "aws_iam_policy_document" "ec2_assume_role" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["ec2.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "runtime_instance" {
  name               = "${local.name_prefix}-runtime-instance"
  assume_role_policy = data.aws_iam_policy_document.ec2_assume_role.json
  tags               = local.common_tags
}

data "aws_iam_policy_document" "runtime_instance" {
  statement {
    sid = "ReadArtifactObjects"
    actions = [
      "s3:GetObject",
      "s3:HeadObject"
    ]
    resources = [local.artifact_objects_arn]
  }

  dynamic "statement" {
    for_each = var.sync_artifacts_back ? [1] : []
    content {
      sid       = "WriteCompiledArtifacts"
      actions   = ["s3:PutObject"]
      resources = [local.artifact_objects_arn]
    }
  }

  statement {
    sid       = "ListArtifactPrefix"
    actions   = ["s3:ListBucket"]
    resources = [local.artifact_bucket_arn]

    condition {
      test     = "StringLike"
      variable = "s3:prefix"
      values   = ["${local.artifact_prefix}*"]
    }
  }

  statement {
    sid = "WriteBootstrapLogs"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents"
    ]
    resources = ["arn:aws:logs:${var.aws_region}:${data.aws_caller_identity.current.account_id}:*"]
  }

  dynamic "statement" {
    for_each = var.hf_token_secret_arn == null ? [] : [var.hf_token_secret_arn]
    content {
      sid       = "ReadHuggingFaceSecret"
      actions   = ["secretsmanager:GetSecretValue"]
      resources = [statement.value]
    }
  }

  dynamic "statement" {
    for_each = var.hf_token_ssm_parameter_name == null ? [] : [var.hf_token_ssm_parameter_name]
    content {
      sid       = "ReadHuggingFaceSsmParameter"
      actions   = ["ssm:GetParameter"]
      resources = ["arn:aws:ssm:${var.aws_region}:${data.aws_caller_identity.current.account_id}:parameter/${trimprefix(statement.value, "/")}"]
    }
  }
}

resource "aws_iam_policy" "runtime_instance" {
  name   = "${local.name_prefix}-runtime-instance"
  policy = data.aws_iam_policy_document.runtime_instance.json
  tags   = local.common_tags
}

resource "aws_iam_role_policy_attachment" "runtime_instance" {
  role       = aws_iam_role.runtime_instance.name
  policy_arn = aws_iam_policy.runtime_instance.arn
}

resource "aws_iam_role_policy_attachment" "runtime_instance_ssm" {
  role       = aws_iam_role.runtime_instance.name
  policy_arn = "arn:aws:iam::aws:policy/AmazonSSMManagedInstanceCore"
}

resource "aws_iam_instance_profile" "runtime_instance" {
  name = "${local.name_prefix}-runtime-instance"
  role = aws_iam_role.runtime_instance.name
}

# Service linked role for SSM is often pre-created in the account.
# Commenting out to avoid "Already Exists" conflicts.
# resource "aws_iam_service_linked_role" "ssm" {
#   aws_service_name = "ssm.amazonaws.com"
# }

data "aws_iam_policy_document" "lambda_assume_role" {
  statement {
    actions = ["sts:AssumeRole"]

    principals {
      type        = "Service"
      identifiers = ["lambda.amazonaws.com"]
    }
  }
}

resource "aws_iam_role" "lambda_wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  name               = "${local.name_prefix}-wakebridge"
  assume_role_policy = data.aws_iam_policy_document.lambda_assume_role.json
  tags               = local.common_tags
}

data "aws_iam_policy_document" "lambda_wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  statement {
    sid       = "ScaleRuntimeAsg"
    actions   = ["autoscaling:SetDesiredCapacity"]
    resources = [aws_autoscaling_group.runtime.arn]
  }

  statement {
    sid = "DescribeRuntimeAsg"
    actions = [
      "autoscaling:DescribeAutoScalingGroups",
      "autoscaling:DescribeWarmPool"
    ]
    resources = ["*"]
  }

  statement {
    sid = "WriteLambdaLogs"
    actions = [
      "logs:CreateLogGroup",
      "logs:CreateLogStream",
      "logs:PutLogEvents"
    ]
    resources = ["arn:aws:logs:${var.aws_region}:${data.aws_caller_identity.current.account_id}:*"]
  }
}

resource "aws_iam_policy" "lambda_wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  name   = "${local.name_prefix}-wakebridge"
  policy = data.aws_iam_policy_document.lambda_wakebridge[0].json
  tags   = local.common_tags
}

resource "aws_iam_role_policy_attachment" "lambda_wakebridge" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  role       = aws_iam_role.lambda_wakebridge[0].name
  policy_arn = aws_iam_policy.lambda_wakebridge[0].arn
}

resource "aws_lambda_permission" "allow_public_invoke" {
  count = var.deploy_lambda_wakebridge ? 1 : 0

  statement_id           = "AllowPublicInvokeFunctionUrl"
  action                 = "lambda:InvokeFunctionUrl"
  function_name          = aws_lambda_function.wakebridge[0].function_name
  principal              = "*"
  function_url_auth_type = "NONE"
}
