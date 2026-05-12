resource "aws_autoscaling_group" "runtime" {
  name                      = "${local.name_prefix}-inf2-asg"
  min_size                  = var.asg_min_size
  desired_capacity          = var.asg_desired_capacity
  max_size                  = var.asg_max_size
  vpc_zone_identifier       = local.public_subnet_ids
  target_group_arns         = [aws_lb_target_group.runtime.arn]
  health_check_type         = "ELB"
  health_check_grace_period = 900
  wait_for_capacity_timeout = "0"
  protect_from_scale_in     = false
  force_delete              = true
  force_delete_warm_pool    = true

  dynamic "launch_template" {
    for_each = var.use_spot_instances ? [] : [1]

    content {
      id      = aws_launch_template.runtime.id
      version = "$Latest"
    }
  }

  dynamic "mixed_instances_policy" {
    for_each = var.use_spot_instances ? [1] : []

    content {
      instances_distribution {
        on_demand_base_capacity                  = 0
        on_demand_percentage_above_base_capacity = 0
        spot_allocation_strategy                 = "capacity-optimized"
      }

      launch_template {
        launch_template_specification {
          launch_template_id = aws_launch_template.runtime.id
          version            = "$Latest"
        }

        dynamic "override" {
          for_each = local.spot_instance_type_overrides

          content {
            instance_type = override.value
          }
        }
      }
    }
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
  count                  = var.enable_idle_scale_down ? 1 : 0
  name                   = "${local.name_prefix}-scale-down"
  scaling_adjustment     = -1
  adjustment_type        = "ChangeInCapacity"
  cooldown               = 300
  autoscaling_group_name = aws_autoscaling_group.runtime.name
}

resource "aws_cloudwatch_metric_alarm" "scale_down" {
  count               = var.enable_idle_scale_down ? 1 : 0
  alarm_name          = "${local.name_prefix}-idle-scale-down"
  comparison_operator = "LessThanOrEqualToThreshold"
  evaluation_periods  = "2"
  datapoints_to_alarm = "2"
  threshold           = "0"
  alarm_description   = "Scale down ASG after 10 minutes with 0 requests, but only after the target is healthy"
  alarm_actions       = [aws_autoscaling_policy.scale_down[0].arn]
  treat_missing_data  = "notBreaching"

  metric_query {
    id          = "idle"
    expression  = "IF(FILL(healthy, 0) > 0, FILL(requests, 0), 1)"
    label       = "Requests while target is healthy"
    return_data = true
  }

  metric_query {
    id          = "requests"
    return_data = false

    metric {
      namespace   = "AWS/ApplicationELB"
      metric_name = "RequestCount"
      period      = 300
      stat        = "Sum"

      dimensions = {
        LoadBalancer = aws_lb.runtime.arn_suffix
        TargetGroup  = aws_lb_target_group.runtime.arn_suffix
      }
    }
  }

  metric_query {
    id          = "healthy"
    return_data = false

    metric {
      namespace   = "AWS/ApplicationELB"
      metric_name = "HealthyHostCount"
      period      = 300
      stat        = "Average"

      dimensions = {
        LoadBalancer = aws_lb.runtime.arn_suffix
        TargetGroup  = aws_lb_target_group.runtime.arn_suffix
      }
    }
  }
}
