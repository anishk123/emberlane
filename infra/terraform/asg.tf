resource "aws_autoscaling_group" "runtime" {
  name                      = "${local.name_prefix}-inf2-asg"
  min_size                  = var.asg_min_size
  desired_capacity          = var.asg_desired_capacity
  max_size                  = var.asg_max_size
  vpc_zone_identifier       = local.public_subnet_ids
  target_group_arns         = [aws_lb_target_group.runtime.arn]
  health_check_type         = "ELB"
  health_check_grace_period = 900
  protect_from_scale_in     = false

  launch_template {
    id      = aws_launch_template.runtime.id
    version = "$Latest"
  }

  dynamic "warm_pool" {
    for_each = var.enable_warm_pool ? [1] : []

    content {
      pool_state                  = var.warm_pool_pool_state
      min_size                    = var.warm_pool_min_size
      max_group_prepared_capacity = var.asg_max_size

      instance_reuse_policy {
        reuse_on_scale_in = true
      }
    }
  }

  tag {
    key                 = "Name"
    value               = "${local.name_prefix}-inf2-runtime"
    propagate_at_launch = true
  }

  tag {
    key                 = "Project"
    value               = var.app_name
    propagate_at_launch = true
  }

  tag {
    key                 = "Environment"
    value               = var.environment
    propagate_at_launch = true
  }

  lifecycle {
    ignore_changes = [desired_capacity]
  }

  depends_on = [terraform_data.input_validation]
}

resource "aws_autoscaling_policy" "scale_down" {
  name                   = "${local.name_prefix}-scale-down"
  scaling_adjustment     = -1
  adjustment_type        = "ChangeInCapacity"
  cooldown               = 300
  autoscaling_group_name = aws_autoscaling_group.runtime.name
}

resource "aws_cloudwatch_metric_alarm" "scale_down" {
  alarm_name          = "${local.name_prefix}-idle-scale-down"
  comparison_operator = "LessThanOrEqualToThreshold"
  evaluation_periods  = "3"
  metric_name         = "RequestCount"
  namespace           = "AWS/ApplicationELB"
  period              = "300"
  statistic           = "Sum"
  threshold           = "0"
  alarm_description   = "Scale down ASG when there are 0 requests for 15 minutes"
  alarm_actions       = [aws_autoscaling_policy.scale_down.arn]
  treat_missing_data  = "breaching"

  dimensions = {
    LoadBalancer = aws_lb.runtime.arn_suffix
  }
}
