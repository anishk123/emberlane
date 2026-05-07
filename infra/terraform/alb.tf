resource "aws_lb" "runtime" {
  name               = "${local.name_prefix}-runtime"
  internal           = !var.public_alb
  load_balancer_type = "application"
  security_groups    = [aws_security_group.alb.id]
  subnets            = local.public_subnet_ids

  tags = merge(local.common_tags, {
    Name = "${local.name_prefix}-runtime-alb"
  })
}

resource "aws_lb_target_group" "runtime" {
  name        = "${local.name_prefix}-runtime"
  port        = var.runtime_port
  protocol    = "HTTP"
  target_type = "instance"
  vpc_id      = local.vpc_id

  health_check {
    enabled             = true
    path                = var.health_path
    matcher             = "200"
    protocol            = "HTTP"
    interval            = 30
    timeout             = 10
    healthy_threshold   = 2
    unhealthy_threshold = 5
  }

  tags = merge(local.common_tags, {
    Name = "${local.name_prefix}-runtime-tg"
  })
}

resource "random_password" "alb_secret" {
  length  = 32
  special = false
}

resource "aws_lb_listener" "http" {
  load_balancer_arn = aws_lb.runtime.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type = "fixed-response"
    fixed_response {
      content_type = "text/plain"
      message_body = "Access Denied"
      status_code  = "403"
    }
  }

  tags = local.common_tags
}

resource "aws_lb_listener_rule" "allow_health" {
  listener_arn = aws_lb_listener.http.arn
  priority     = 1

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.runtime.arn
  }

  condition {
    path_pattern {
      values = [var.health_path]
    }
  }
}

resource "aws_lb_listener_rule" "allow_secret" {
  listener_arn = aws_lb_listener.http.arn
  priority     = 10

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.runtime.arn
  }

  condition {
    http_header {
      http_header_name = "X-Emberlane-Secret"
      values           = [random_password.alb_secret.result]
    }
  }
}
