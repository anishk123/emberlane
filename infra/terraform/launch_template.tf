resource "aws_launch_template" "runtime" {
  name_prefix   = "${local.name_prefix}-inf2-"
  image_id      = var.ami_id != "" ? var.ami_id : "ami-00000000000000000"
  instance_type = var.instance_type
  key_name      = var.key_name

  iam_instance_profile {
    name = aws_iam_instance_profile.runtime_instance.name
  }

  vpc_security_group_ids = [aws_security_group.instance.id]

  block_device_mappings {
    device_name = "/dev/sda1"

    ebs {
      volume_size           = var.root_volume_size_gb
      volume_type           = "gp3"
      encrypted             = true
      delete_on_termination = true
    }
  }

  metadata_options {
    http_endpoint               = "enabled"
    http_tokens                 = "required"
    http_put_response_hop_limit = 2
  }

  user_data = base64encode(templatefile("${path.module}/user_data.sh.tftpl", {
    model_profile               = var.model_profile
    model_id                    = var.model_id
    accelerator                 = var.accelerator
    runtime_pack                = var.runtime_pack
    artifact_bucket             = local.artifact_bucket_name
    artifact_prefix             = local.artifact_prefix
    s3_neuron_artifacts_uri     = local.s3_neuron_artifacts_uri
    sync_artifacts_back         = tostring(var.sync_artifacts_back)
    runtime_port                = tostring(var.runtime_port)
    aws_region                  = var.aws_region
    hf_token_secret_arn         = var.hf_token_secret_arn == null ? "" : var.hf_token_secret_arn
    hf_token_ssm_parameter_name = var.hf_token_ssm_parameter_name == null ? "" : var.hf_token_ssm_parameter_name
    use_baked_ami               = tostring(var.use_baked_ami)
    runtime_pack_repo_url       = var.runtime_pack_repo_url
    runtime_pack_git_ref        = var.runtime_pack_git_ref
    runtime_pack_s3_uri         = "s3://${local.artifact_bucket_name}/${aws_s3_object.inf2_runtime_pack[0].key}"
  }))

  tag_specifications {
    resource_type = "instance"
    tags = merge(local.common_tags, {
      Name = "${local.name_prefix}-inf2-runtime"
    })
  }

  tag_specifications {
    resource_type = "volume"
    tags = merge(local.common_tags, {
      Name = "${local.name_prefix}-inf2-runtime-root"
    })
  }

  tags = merge(local.common_tags, {
    Name = "${local.name_prefix}-inf2-launch-template"
  })

  depends_on = [terraform_data.input_validation]
}
