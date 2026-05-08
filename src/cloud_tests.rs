use crate::{
    cloud::{
        aws::terraform_install_help,
        future,
        model::{Accelerator, CloudBackend, CloudProvider},
        profiles, AwsBackend, CostMode,
    },
    error::EmberlaneError,
};
use std::{fs, path::PathBuf, str::FromStr};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).unwrap()
}

#[test]
fn cloud_provider_parses_and_future_clouds_are_not_implemented() {
    assert_eq!(CloudProvider::from_str("aws").unwrap(), CloudProvider::Aws);
    assert_eq!(CloudProvider::from_str("gcp").unwrap(), CloudProvider::Gcp);
    assert_eq!(
        CloudProvider::from_str("azure").unwrap(),
        CloudProvider::Azure
    );
    assert!(future::not_implemented("gcp")
        .to_string()
        .contains("GCP backend is not implemented yet"));
    assert!(future::not_implemented("azure")
        .to_string()
        .contains("Azure backend is not implemented yet"));
}

#[test]
fn model_profiles_parse_and_include_cuda_first_profiles() {
    let profiles = profiles::all_profiles().unwrap();
    let qwen = profiles.get("qwen35_9b").unwrap();
    assert_eq!(qwen.default_accelerator, "cuda");
    assert_eq!(qwen.recommended_instance, "g5.2xlarge");
    assert_eq!(qwen.status, "recommended");
    assert!(qwen.language_model_only);
    assert_eq!(qwen.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(
        qwen.fallback_instances,
        vec!["g5.4xlarge".to_string(), "g5.8xlarge".to_string()]
    );
    assert_eq!(qwen.max_model_len, 4096);
    assert_eq!(profiles.get("llama32_1b_inf2").unwrap().status, "stable");
    assert!(profiles
        .get("llama32_1b_inf2_economy")
        .unwrap()
        .display_name
        .contains("Tight Memory"));
    assert!(profiles
        .get("qwen25_15b_inf2_economy")
        .unwrap()
        .display_name
        .contains("Tight Memory"));
}

#[test]
fn cost_modes_map_to_terraform_values() {
    assert_eq!(
        CostMode::Economy.terraform_values()["enable_warm_pool"],
        false
    );
    assert_eq!(
        CostMode::Economy.terraform_values()["use_spot_instances"],
        true
    );
    assert_eq!(
        CostMode::Economy.terraform_values()["enable_idle_scale_down"],
        true
    );
    assert_eq!(
        CostMode::Balanced.terraform_values()["enable_warm_pool"],
        false
    );
    assert_eq!(
        CostMode::Balanced.terraform_values()["asg_desired_capacity"],
        1
    );
    assert_eq!(
        CostMode::Balanced.terraform_values()["use_spot_instances"],
        false
    );
    assert_eq!(
        CostMode::Balanced.terraform_values()["enable_idle_scale_down"],
        true
    );
    assert_eq!(
        CostMode::AlwaysOn.terraform_values()["asg_desired_capacity"],
        1
    );
    assert_eq!(
        CostMode::AlwaysOn.terraform_values()["use_spot_instances"],
        false
    );
    assert_eq!(
        CostMode::AlwaysOn.terraform_values()["enable_idle_scale_down"],
        false
    );
    assert_eq!(
        CostMode::AlwaysOn.terraform_values()["desired_capacity_on_sleep"],
        1
    );
}

#[tokio::test]
async fn aws_backend_renders_cuda_and_inf2_tfvars() {
    let cuda = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen35_9b".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("balanced".to_string()),
            None,
        )
        .unwrap();
    let vars = cuda.render_deploy_vars().await.unwrap();
    assert_eq!(vars["accelerator"], "cuda");
    assert_eq!(vars["runtime_pack"], "cuda-vllm");
    assert_eq!(vars["enable_warm_pool"], false);
    assert_eq!(vars["enable_idle_scale_down"], true);
    assert_eq!(vars["use_spot_instances"], false);
    assert_eq!(vars["desired_capacity_on_wake"], 1);
    assert_eq!(vars["desired_capacity_on_sleep"], 0);
    assert_eq!(vars["model_id"], "Qwen/Qwen3.5-9B");
    assert_eq!(vars["max_model_len"], 4096);
    assert_eq!(vars["language_model_only"], true);
    assert_eq!(vars["reasoning_parser"], "qwen3");

    let inf2 = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("llama32_1b_inf2".to_string()),
            Some("inf2".to_string()),
            Some("inf2.8xlarge".to_string()),
            Some("balanced".to_string()),
            None,
        )
        .unwrap();
    let vars = inf2.render_deploy_vars().await.unwrap();
    assert_eq!(vars["accelerator"], "inf2");
    assert_eq!(vars["runtime_pack"], "inf2-neuron");
    assert_eq!(vars["enable_warm_pool"], false);
}

#[tokio::test]
async fn aws_backend_renders_direct_deploy_profile_region_and_ami() {
    let mut backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen35_9b".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("balanced".to_string()),
            None,
        )
        .unwrap();
    backend.config.profile = Some("emberlane".to_string());
    backend.config.region = "us-west-2".to_string();
    backend.config.ami_id = "ami-1234567890abcdef0".to_string();
    let vars = backend.render_deploy_vars().await.unwrap();
    assert_eq!(vars["aws_profile"], "emberlane");
    assert_eq!(vars["aws_region"], "us-west-2");
    assert_eq!(vars["ami_id"], "ami-1234567890abcdef0");
    assert_eq!(vars["enable_warm_pool"], false);
    assert_eq!(vars["enable_idle_scale_down"], true);
    assert_eq!(vars["use_spot_instances"], false);
    assert_eq!(vars["desired_capacity_on_sleep"], 0);
    assert_eq!(vars["max_model_len"], 4096);
    assert_eq!(vars["language_model_only"], true);
    assert_eq!(vars["reasoning_parser"], "qwen3");

    let always_on = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen35_9b".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("always-on".to_string()),
            None,
        )
        .unwrap();
    let vars = always_on.render_deploy_vars().await.unwrap();
    assert_eq!(vars["asg_desired_capacity"], 1);
    assert_eq!(vars["enable_idle_scale_down"], false);
    assert_eq!(vars["desired_capacity_on_sleep"], 1);
}

#[test]
fn terraform_missing_message_is_actionable() {
    let help = terraform_install_help();
    assert!(help.contains("brew install"));
    assert!(help.contains("terraform version"));
    assert!(help.contains("--profile emberlane"));
    assert!(help.contains("--plan-only"));
}

#[tokio::test]
async fn aws_backend_doctor_and_cost_report_are_honest() {
    let backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("llama32_1b_inf2".to_string()),
            Some("inf2".to_string()),
            Some("inf2.xlarge".to_string()),
            Some("always-on".to_string()),
            None,
        )
        .unwrap();
    let doctor = backend.doctor().await.unwrap();
    assert!(doctor["warnings"].to_string().contains("experimental"));
    assert!(doctor["warnings"]
        .to_string()
        .contains("recommends instance"));
    assert_eq!(doctor["capacity"]["skipped"], true);
    let cost = backend.cost_report().await.unwrap();
    assert_eq!(cost["pricing_configured"], false);
    assert_eq!(cost["savings_claimed"], false);
}

#[test]
fn aws_init_config_text_is_cuda_first() {
    let text = AwsBackend::default_config_text().unwrap();
    assert!(text.contains("accelerator = \"cuda\""));
    assert!(text.contains("instance_type = \"g5.2xlarge\""));
    assert!(text.contains("model_profile = \"qwen35_9b\""));
    assert!(text.contains("mode = \"balanced\""));
    assert!(text.contains("max_model_len = 4096"));
    assert!(text.contains("language_model_only = true"));
    assert!(text.contains("reasoning_parser = \"qwen3\""));
}

#[test]
fn examples_and_readme_are_cleaned_to_active_surface() {
    let entries = fs::read_dir(root().join("examples"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert!(entries.contains(&"echo-runtime".to_string()));
    assert!(entries.contains(&"simple-chat".to_string()));
    for forbidden in ["rag-worker", "search-worker", "file-worker", "plugins"] {
        assert!(!entries.contains(&forbidden.to_string()));
    }
    let readme = read("README.md");
    for forbidden in ["plugin marketplace", "multi-agent", "workflow builder"] {
        assert!(!readme.to_lowercase().contains(forbidden));
    }
    assert!(readme.contains("Supported Interfaces"));
    assert!(readme.contains("How Defaults Work"));
    assert!(readme.contains(
        "CLI for local setup, AWS deploy, benchmarking, cost reports, diagnostics, and cleanup"
    ));
    assert!(readme.contains("MCP stdio"));
    assert!(readme.contains("OpenAI-compatible chat endpoints"));
    assert!(readme.contains("AWS Quickstart"));
    assert!(readme.contains("AWS Terraform deployment"));
    assert!(readme.contains("aws print-config"));
    assert!(readme.contains("File Storage And Multi-Document Chat"));
    assert!(readme.contains("Planned"));
    assert!(readme.contains("Python SDK"));
    assert!(readme.contains("TypeScript SDK"));
    assert!(readme.contains("Not Implemented Yet"));
    assert!(readme.contains("Architecture"));
    assert!(readme.contains("Implemented Now"));
    assert!(readme.contains("recommended first path"));
    assert!(readme.contains("tighter-memory model profiles, not the AWS cost mode named `economy`"));
}

#[test]
fn docs_state_future_clouds_are_not_implemented() {
    let future = read("docs/future-clouds.md");
    assert!(future.contains("GCP and Azure are planned but not implemented"));
    assert!(future.contains("AWS is the first implemented"));
}

#[test]
fn terraform_accepts_model_mode_runtime_pack_variables() {
    let vars = read("infra/terraform/variables.tf");
    for name in [
        "accelerator",
        "runtime_pack",
        "model_profile",
        "model_id",
        "mode",
    ] {
        assert!(vars.contains(&format!("variable \"{name}\"")));
    }
    assert!(read("infra/terraform/asg.tf").contains("ignore_changes = [desired_capacity]"));
}

#[test]
fn sdk_surface_is_planned_not_supported() {
    assert!(!root().join("sdks").exists());
    let readme = read("README.md");
    assert!(readme.contains("Planned"));
    assert!(readme.contains("Python SDK"));
    assert!(readme.contains("TypeScript SDK"));
    assert!(readme.contains("Not Implemented Yet"));
    assert!(!readme.contains("pip install emberlane"));
    assert!(!readme.contains("npm install emberlane"));

    let future = read("docs/roadmap/future-work.md");
    assert!(future.contains("Planned: Python And TypeScript SDKs"));
    let checklist = read("docs/completeness-checklist.md");
    assert!(checklist.contains("CLI"));
    assert!(checklist.contains("MCP"));
    assert!(checklist.contains("HTTP/OpenAI-compatible API"));
    assert!(checklist.contains("Python SDK"));
    assert!(checklist.contains("TypeScript SDK"));
}

#[test]
fn unsupported_accelerator_errors_cleanly() {
    let err = Accelerator::from_str("tpu").unwrap_err();
    assert!(err.contains("not supported"));
    let err = CloudProvider::from_str("gcp2").unwrap_err();
    assert!(matches!(
        EmberlaneError::InvalidRequest(err.clone()),
        EmberlaneError::InvalidRequest(_)
    ));
}
