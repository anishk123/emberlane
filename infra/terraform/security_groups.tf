resource "aws_security_group" "alb" {
  name        = "${local.name_prefix}-alb"
  description = "Dev HTTP ingress for Emberlane Inf2 ALB"
  vpc_id      = local.vpc_id

  ingress {
    description = "Dev HTTP access. Restrict allowed_ingress_cidr_blocks before production use."
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = var.allowed_ingress_cidr_blocks
  }

  egress {
    description = "Outbound to runtime targets"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.common_tags, {
    Name = "${local.name_prefix}-alb-sg"
  })
}

resource "aws_security_group" "instance" {
  name        = "${local.name_prefix}-instance"
  description = "Inf2 runtime instance security group"
  vpc_id      = local.vpc_id

  ingress {
    description     = "Runtime traffic from ALB only"
    from_port       = var.runtime_port
    to_port         = var.runtime_port
    protocol        = "tcp"
    security_groups = [aws_security_group.alb.id]
  }

  dynamic "ingress" {
    for_each = var.key_name != null && length(var.allowed_ssh_cidr_blocks) > 0 ? [1] : []
    content {
      description = "Optional SSH for break-glass dev debugging"
      from_port   = 22
      to_port     = 22
      protocol    = "tcp"
      cidr_blocks = var.allowed_ssh_cidr_blocks
    }
  }

  egress {
    description = "Outbound downloads, package installs, S3, CloudWatch, and model fetches"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = merge(local.common_tags, {
    Name = "${local.name_prefix}-instance-sg"
  })
}
