resource "terraform_data" "input_validation" {
  input = {
    ami_id         = var.ami_id
    create_vpc     = var.create_vpc
    public_subnets = local.public_subnet_ids
  }

  lifecycle {
    precondition {
      condition     = var.create_vpc || (var.vpc_id != null && length(var.public_subnet_ids) >= 2)
      error_message = "When create_vpc=false, set vpc_id and at least two public_subnet_ids."
    }

    precondition {
      condition     = !var.create_vpc || length(var.public_subnet_cidrs) <= length(data.aws_availability_zones.available.names)
      error_message = "Not enough available AZs for the requested public_subnet_cidrs. Reduce the subnet count or choose a region with more available AZs."
    }

    precondition {
      condition     = var.ami_id != ""
      error_message = "ami_id is required. Use an AWS Neuron Deep Learning AMI or a baked Emberlane Inf2 AMI."
    }

    precondition {
      condition     = contains(["fast", "slow"], lower(var.wake_mode))
      error_message = "wake_mode must be fast or slow."
    }
  }
}
