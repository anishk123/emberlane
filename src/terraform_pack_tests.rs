use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap()
}

#[test]
fn terraform_pack_files_exist() {
    for path in [
        "infra/terraform/README.md",
        "infra/terraform/versions.tf",
        "infra/terraform/providers.tf",
        "infra/terraform/variables.tf",
        "infra/terraform/locals.tf",
        "infra/terraform/main.tf",
        "infra/terraform/outputs.tf",
        "infra/terraform/vpc.tf",
        "infra/terraform/security_groups.tf",
        "infra/terraform/iam.tf",
        "infra/terraform/s3.tf",
        "infra/terraform/alb.tf",
        "infra/terraform/launch_template.tf",
        "infra/terraform/asg.tf",
        "infra/terraform/warm_pool.tf",
        "infra/terraform/lambda.tf",
        "infra/terraform/user_data.sh.tftpl",
        "infra/terraform/terraform.tfvars.example",
        "infra/terraform/tests/README.md",
        "docs/aws-deploy-from-zero.md",
    ] {
        assert!(root().join(path).exists(), "{path} should exist");
    }
}

#[test]
fn variables_include_required_inputs() {
    let variables = read("infra/terraform/variables.tf");
    for name in [
        "app_name",
        "aws_region",
        "aws_profile",
        "environment",
        "create_vpc",
        "vpc_id",
        "public_subnet_ids",
        "private_subnet_ids",
        "vpc_cidr",
        "public_subnet_cidrs",
        "allowed_ingress_cidr_blocks",
        "public_alb",
        "instance_type",
        "ami_id",
        "key_name",
        "model_profile",
        "model_id",
        "max_model_len",
        "language_model_only",
        "reasoning_parser",
        "accelerator",
        "runtime_pack",
        "mode",
        "runtime_port",
        "health_path",
        "root_volume_size_gb",
        "use_baked_ami",
        "asg_min_size",
        "asg_desired_capacity",
        "asg_max_size",
        "desired_capacity_on_wake",
        "desired_capacity_on_sleep",
        "enable_idle_scale_down",
        "enable_warm_pool",
        "warm_pool_min_size",
        "warm_pool_pool_state",
        "deploy_lambda_wakebridge",
        "lambda_runtime",
        "lambda_timeout_secs",
        "lambda_memory_mb",
        "wake_mode",
        "fast_wait_secs",
        "startup_timeout_secs",
        "retry_after_secs",
        "api_key",
        "function_url_auth_type",
        "create_artifact_bucket",
        "artifact_bucket_name",
        "artifact_prefix",
        "enable_s3_artifact_sync",
        "sync_artifacts_back",
        "hf_token_secret_arn",
        "hf_token_ssm_parameter_name",
        "neuron_artifacts_s3_prefix",
        "log_retention_days",
    ] {
        assert!(
            variables.contains(&format!("variable \"{name}\"")),
            "missing variable {name}"
        );
    }
}

#[test]
fn terraform_resources_include_required_wakebridge_pieces() {
    let versions = read("infra/terraform/versions.tf");
    assert!(versions.contains(">= 1.6.0"));
    assert!(versions.contains("hashicorp/aws"));
    assert!(versions.contains(">= 5.0"));

    let asg = read("infra/terraform/asg.tf");
    assert!(asg.contains("aws_autoscaling_group"));
    assert!(asg.contains("ignore_changes"));
    assert!(asg.contains("desired_capacity"));

    let locals = read("infra/terraform/locals.tf");
    assert!(locals.contains("public_subnet_az_offset"));
    assert!(locals.contains("public_subnet_zone_names"));

    let vpc = read("infra/terraform/vpc.tf");
    assert!(vpc.contains("local.public_subnet_zone_names"));

    let lambda = read("infra/terraform/lambda.tf");
    assert!(lambda.contains("aws_lambda_function_url"));
    assert!(lambda.contains("RESPONSE_STREAM"));
    assert!(lambda.contains("BASE_URL"));
    assert!(lambda.contains("ASG_NAME"));

    let iam = read("infra/terraform/iam.tf");
    assert!(iam.contains("autoscaling:SetDesiredCapacity"));
    assert!(iam.contains("autoscaling:DescribeAutoScalingGroups"));
    assert!(iam.contains("autoscaling:DescribeWarmPool"));

    let user_data = read("infra/terraform/user_data.sh.tftpl");
    assert!(user_data.contains("MODEL_PROFILE"));
    assert!(user_data.contains("ARTIFACT_BUCKET"));
    assert!(user_data.contains("S3_NEURON_ARTIFACTS_URI"));
    assert!(user_data.contains("MAX_MODEL_LEN"));
    assert!(user_data.contains("LANGUAGE_MODEL_ONLY"));
    assert!(user_data.contains("REASONING_PARSER"));
    assert!(user_data.contains("HF_HOME=/opt/emberlane/model-cache"));
    assert!(user_data.contains("TRANSFORMERS_CACHE=/opt/emberlane/model-cache"));
    assert!(user_data.contains("safetensors prefetch"));
    assert!(user_data.contains("systemctl enable --now emberlane-runtime.service"));
    assert!(user_data.contains("docker run --rm --name emberlane-vllm"));
}

#[test]
fn terraform_docs_and_scripts_are_operator_ready() {
    let docs = read("docs/aws-deploy-from-zero.md");
    assert!(docs.contains("terraform apply"));
    assert!(docs.contains("terraform destroy"));
    assert!(docs.contains("Cost Warning"));
    assert!(docs.contains("Inf2 quota"));
    assert!(docs.contains("Lambda Function URL response streaming"));

    let readme = read("README.md");
    assert!(readme.contains("AWS Quickstart"));
    assert!(readme.contains("AWS Terraform deployment"));
    assert!(readme.contains("docs/aws-deploy-from-zero.md"));

    for script in [
        "aws/scripts/deploy-dev.sh",
        "aws/scripts/destroy-dev.sh",
        "aws/scripts/smoke-test-lambda.sh",
        "aws/scripts/smoke-test-streaming.sh",
        "aws/scripts/wait-for-health.sh",
        "aws/scripts/scale-down.sh",
        "aws/scripts/check-asg.sh",
    ] {
        let body = read(script);
        assert!(
            body.starts_with("#!/usr/bin/env bash"),
            "{script} missing shebang"
        );
        assert!(
            body.contains("set -euo pipefail"),
            "{script} missing safety flags"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(root().join(script))
                .unwrap()
                .permissions()
                .mode();
            assert_ne!(mode & 0o111, 0, "{script} should be executable");
        }
    }
}
