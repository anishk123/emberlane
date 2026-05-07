variable "app_name" {
  description = "Application name used for tags and resource names."
  type        = string
  default     = "emberlane"
}

variable "aws_region" {
  description = "AWS region for the deployment."
  type        = string
  default     = "us-west-2"
}

variable "aws_profile" {
  description = "Optional AWS CLI/profile name for Terraform operations."
  type        = string
  default     = null
}

variable "environment" {
  description = "Environment label used in names and tags."
  type        = string
  default     = "dev"
}

variable "emberlane_test_run" {
  description = "Optional Emberlane integration-test run ID for resource tagging."
  type        = string
  default     = ""
}

variable "create_vpc" {
  description = "Create a small public VPC for dev/test. Set false to use existing networking."
  type        = bool
  default     = true
}

variable "vpc_id" {
  description = "Existing VPC ID when create_vpc is false."
  type        = string
  default     = null
}

variable "public_subnet_ids" {
  description = "Existing public subnet IDs when create_vpc is false."
  type        = list(string)
  default     = []
}

variable "private_subnet_ids" {
  description = "Reserved for future/private deployments. Not used by the default public ALB path."
  type        = list(string)
  default     = []
}

variable "vpc_cidr" {
  description = "CIDR for the generated dev VPC."
  type        = string
  default     = "10.42.0.0/16"
}

variable "public_subnet_cidrs" {
  description = "CIDRs for generated public subnets."
  type        = list(string)
  default     = ["10.42.1.0/24", "10.42.2.0/24"]
}

variable "allowed_ingress_cidr_blocks" {
  description = "CIDRs allowed to reach the dev HTTP ALB. 0.0.0.0/0 is dev-only; restrict this before real use."
  type        = list(string)
  default     = ["0.0.0.0/0"]
}

variable "allowed_ssh_cidr_blocks" {
  description = "Optional SSH CIDRs. Leave empty to disable SSH ingress even when key_name is set."
  type        = list(string)
  default     = []
}

variable "public_alb" {
  description = "Create an internet-facing ALB. The default Lambda Function URL streaming path expects this."
  type        = bool
  default     = true
}

variable "instance_type" {
  description = "AWS instance type for the runtime ASG."
  type        = string
  default     = "g5.xlarge"
}

variable "ami_id" {
  description = "AMI ID for the runtime. Use a GPU Deep Learning AMI for cuda-vllm or a Neuron Deep Learning/baked AMI for inf2-neuron. Required for apply."
  type        = string
  default     = ""
}

variable "key_name" {
  description = "Optional EC2 key pair name. SSH ingress is still disabled unless allowed_ssh_cidr_blocks is set."
  type        = string
  default     = null
}

variable "model_profile" {
  description = "Emberlane model profile from profiles/models.toml."
  type        = string
  default     = "llama31_8b"
}

variable "model_id" {
  description = "Resolved Hugging Face model ID for the runtime."
  type        = string
  default     = ""
}

variable "accelerator" {
  description = "Accelerator family: cuda or inf2."
  type        = string
  default     = "cuda"
}

variable "runtime_pack" {
  description = "Runtime pack to start on the instance: cuda-vllm or inf2-neuron."
  type        = string
  default     = "cuda-vllm"
}

variable "mode" {
  description = "Emberlane cost mode: economy, balanced, or always-on."
  type        = string
  default     = "economy"
}

variable "runtime_port" {
  description = "Port exposed by nginx/Inf2 runtime to the ALB target group."
  type        = number
  default     = 8080
}

variable "health_path" {
  description = "ALB and WakeBridge health path."
  type        = string
  default     = "/health"
}

variable "root_volume_size_gb" {
  description = "Root EBS volume size. Model downloads and Neuron artifacts need room."
  type        = number
  default     = 200
}

variable "use_baked_ami" {
  description = "If true, user-data only writes env and starts the baked service. If false, it attempts dev bootstrap install."
  type        = bool
  default     = false
}

variable "asg_min_size" {
  description = "ASG min size."
  type        = number
  default     = 0
}

variable "asg_desired_capacity" {
  description = "Initial ASG desired capacity. Terraform ignores later desired_capacity drift so WakeBridge can scale."
  type        = number
  default     = 0
}

variable "asg_max_size" {
  description = "ASG max size for dev/test."
  type        = number
  default     = 1
}

variable "desired_capacity_on_wake" {
  description = "ASG desired capacity requested by WakeBridge."
  type        = number
  default     = 1
}

variable "desired_capacity_on_sleep" {
  description = "Desired capacity used by scale-down scripts and Emberlane aws_asg sleep."
  type        = number
  default     = 0
}

variable "enable_warm_pool" {
  description = "Create an ASG Warm Pool for prepared instances."
  type        = bool
  default     = true
}

variable "warm_pool_min_size" {
  description = "Minimum prepared warm pool instances."
  type        = number
  default     = 1
}

variable "warm_pool_pool_state" {
  description = "Warm Pool state, usually Stopped or Hibernated."
  type        = string
  default     = "Stopped"
}

variable "deploy_lambda_wakebridge" {
  description = "Deploy the Lambda WakeBridge Function URL."
  type        = bool
  default     = true
}

variable "lambda_runtime" {
  description = "Lambda runtime. The default uses the Node streaming bridge."
  type        = string
  default     = "nodejs20.x"
}

variable "lambda_timeout_secs" {
  description = "Lambda timeout in seconds."
  type        = number
  default     = 60
}

variable "lambda_memory_mb" {
  description = "Lambda memory size."
  type        = number
  default     = 512
}

variable "wake_mode" {
  description = "WakeBridge mode: fast waits briefly, slow returns warming quickly."
  type        = string
  default     = "fast"
}

variable "fast_wait_secs" {
  description = "Seconds the Lambda WakeBridge waits in fast mode."
  type        = number
  default     = 25
}

variable "startup_timeout_secs" {
  description = "Upper startup timeout passed to WakeBridge."
  type        = number
  default     = 300
}

variable "retry_after_secs" {
  description = "Retry-After value for warming responses."
  type        = number
  default     = 5
}

variable "api_key" {
  description = "Optional bearer token for Lambda WakeBridge. Strongly recommended when function_url_auth_type is NONE."
  type        = string
  default     = null
  sensitive   = true
}

variable "function_url_auth_type" {
  description = "Lambda Function URL auth type. NONE is convenient for dev only."
  type        = string
  default     = "NONE"
}

variable "create_artifact_bucket" {
  description = "Create an S3 artifact bucket for uploads and Neuron cache sync."
  type        = bool
  default     = true
}

variable "artifact_bucket_name" {
  description = "Optional existing or explicit artifact bucket name."
  type        = string
  default     = null
}

variable "artifact_prefix" {
  description = "S3 prefix for Emberlane artifacts."
  type        = string
  default     = "emberlane/"
}

variable "enable_s3_artifact_sync" {
  description = "Pass an S3 Neuron artifact cache URI to the Inf2 runtime."
  type        = bool
  default     = true
}

variable "sync_artifacts_back" {
  description = "Allow the instance role and runtime scripts to sync compiled artifacts back to S3."
  type        = bool
  default     = false
}

variable "hf_token_secret_arn" {
  type        = string
  default     = null
  nullable    = true
  description = "Optional Secrets Manager ARN containing Hugging Face token."
}

variable "hf_token_ssm_parameter_name" {
  type        = string
  default     = null
  nullable    = true
  description = "Optional SSM parameter name containing Hugging Face token."
}

variable "neuron_artifacts_s3_prefix" {
  description = "Prefix under artifact_prefix for Neuron compiled artifacts."
  type        = string
  default     = "neuron-artifacts/"
}

variable "runtime_pack_repo_url" {
  description = "Repository URL used by dev bootstrap mode to fetch the Inf2 runtime pack."
  type        = string
  default     = "https://github.com/emberlane/emberlane.git"
}

variable "runtime_pack_git_ref" {
  description = "Git ref used by dev bootstrap mode."
  type        = string
  default     = "main"
}

variable "log_retention_days" {
  description = "CloudWatch log retention for Lambda logs."
  type        = number
  default     = 14
}
