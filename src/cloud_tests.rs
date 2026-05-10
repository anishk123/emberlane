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
fn model_profiles_parse_and_include_new_public_profiles() {
    let profiles = profiles::all_profiles().unwrap();

    let qwen = profiles.get("qwen3_8b_awq_32k_g5").unwrap();
    assert_eq!(qwen.default_accelerator, "cuda");
    assert_eq!(qwen.recommended_instance, "g5.2xlarge");
    assert_eq!(qwen.status, "recommended");
    assert_eq!(qwen.quantization.as_deref(), Some("awq"));
    assert_eq!(qwen.default_mode.as_deref(), Some("economy"));
    assert_eq!(qwen.default_pricing.as_deref(), Some("spot"));
    assert_eq!(qwen.balanced_pricing.as_deref(), Some("on_demand"));
    assert_eq!(qwen.visibility.as_deref(), Some("recommended"));
    assert_eq!(
        qwen.validation_status.as_deref(),
        Some("needs_emberlane_validation")
    );
    assert!(qwen.require_user_acknowledgement_if_unvalidated);
    assert!(qwen.language_model_only);
    assert_eq!(qwen.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(qwen.serving_modality.as_deref(), Some("text"));
    assert_eq!(qwen.max_model_len, 32768);

    let qwen_g6 = profiles.get("qwen3_8b_awq_32k").unwrap();
    assert_eq!(qwen_g6.visibility.as_deref(), Some("hidden"));
    assert_eq!(qwen_g6.status, "hidden");
    assert_eq!(qwen_g6.recommended_instance, "g6e.xlarge");
    assert_eq!(qwen_g6.serving_modality.as_deref(), Some("text"));

    let qwen128 = profiles.get("qwen3_8b_awq_128k").unwrap();
    assert_eq!(qwen128.quantization.as_deref(), Some("awq"));
    assert_eq!(qwen128.max_model_len, 131072);
    assert_eq!(qwen128.rope_scaling.as_ref().unwrap().rope_type, "yarn");
    assert_eq!(qwen128.safe_instance.as_deref(), Some("g6e.4xlarge"));
    assert_eq!(qwen128.serving_modality.as_deref(), Some("text"));

    let gemma = profiles.get("gemma3_12b_128k").unwrap();
    assert_eq!(gemma.recommended_instance, "g6e.2xlarge");
    assert_eq!(gemma.visibility.as_deref(), Some("advanced"));
    assert_eq!(gemma.serving_modality.as_deref(), Some("multimodal"));
    assert_eq!(
        gemma.note.as_deref(),
        Some("May require Hugging Face access/license acceptance.")
    );

    let deepseek = profiles.get("deepseek_r1_distill_qwen14b_64k").unwrap();
    assert_eq!(deepseek.status, "advanced");
    assert_eq!(deepseek.max_model_len, 65536);
    assert_eq!(deepseek.task_group.as_deref(), Some("Coding - Hard"));
    assert_eq!(deepseek.serving_modality.as_deref(), Some("text"));

    let qwen3 = profiles.get("qwen3_4b_inf2").unwrap();
    assert_eq!(qwen3.visibility.as_deref(), Some("hidden"));
    assert_eq!(qwen3.default_accelerator, "inf2");
    assert_eq!(qwen3.recommended_instance, "inf2.xlarge");
    assert_eq!(qwen3.runtime, "vllm-neuron");
    assert_eq!(qwen3.status, "inf2_experimental");
    assert_eq!(qwen3.model_id, "Qwen/Qwen3-4B-Instruct-2507");
    assert_eq!(qwen3.max_model_len, 2048);
    assert_eq!(qwen3.serving_modality.as_deref(), Some("text"));

    let qwen3_inf2 = profiles.get("qwen3_8b_inf2_4k").unwrap();
    assert_eq!(qwen3_inf2.default_accelerator, "inf2");
    assert_eq!(qwen3_inf2.recommended_instance, "inf2.xlarge");
    assert_eq!(qwen3_inf2.runtime, "vllm-neuron");
    assert_eq!(qwen3_inf2.status, "optional");
    assert_eq!(qwen3_inf2.max_model_len, 4096);
    assert_eq!(qwen3_inf2.safe_instance.as_deref(), Some("inf2.8xlarge"));
    assert_eq!(
        qwen3_inf2.fallback_instances,
        vec!["inf2.8xlarge".to_string()]
    );
    assert_eq!(qwen3_inf2.visibility.as_deref(), Some("optional"));
    assert_eq!(
        qwen3_inf2.validation_status.as_deref(),
        Some("experimental")
    );
    assert!(!qwen3_inf2.require_user_acknowledgement_if_unvalidated);
    assert_eq!(qwen3_inf2.serving_modality.as_deref(), Some("text"));
    assert_eq!(qwen3_inf2.max_num_seqs, Some(8));
    assert_eq!(qwen3_inf2.block_size, Some(32));
    assert_eq!(qwen3_inf2.num_gpu_blocks_override, Some(8));
    assert_eq!(
        qwen3_inf2.vllm_extra_args,
        vec![
            "--device".to_string(),
            "neuron".to_string(),
            "--tensor-parallel-size".to_string(),
            "2".to_string(),
            "--max-num-seqs".to_string(),
            "8".to_string(),
            "--block-size".to_string(),
            "32".to_string(),
            "--num-gpu-blocks-override".to_string(),
            "8".to_string(),
            "--no-enable-prefix-caching".to_string()
        ]
    );

    assert!(!profiles.keys().any(|name| name.ends_with("_economy")));
    assert!(!profiles.keys().any(|name| name.ends_with("_candidate")));
}

#[test]
fn hidden_profiles_do_not_show_in_public_rows() {
    let rows = profiles::rows().unwrap();
    assert!(rows.iter().all(|row| row["profile"] != "qwen35_9b"));
    assert!(rows
        .iter()
        .all(|row| row["profile"] != "qwen35_9b_quantized"));
    assert!(rows.iter().all(|row| row["profile"] != "llama31_8b"));
    assert!(rows.iter().all(|row| row["profile"] != "qwen3_8b_awq_32k"));
    assert!(rows.iter().any(|row| row["profile"] == "qwen3_8b_inf2_4k"));
    let sections = profiles::menu_sections(false).unwrap();
    assert!(sections
        .iter()
        .any(|section| section["task_group"] == "Coding - Simple"));
    assert!(sections
        .iter()
        .any(|section| section["task_group"] == "Research - Deep"));
}

#[test]
fn public_model_menu_prioritizes_recommended_and_shows_instances_clearly() {
    let sections = profiles::menu_sections(false).unwrap();
    assert_eq!(
        sections
            .first()
            .and_then(|section| section["task_group"].as_str()),
        Some("Coding - Simple")
    );
    assert_eq!(
        sections
            .first()
            .and_then(|section| section["profiles"].as_array())
            .and_then(|profiles| profiles.first())
            .and_then(|profile| profile["profile"].as_str()),
        Some("qwen3_8b_awq_32k_g5")
    );
    assert_eq!(
        sections
            .first()
            .and_then(|section| section["profiles"].as_array())
            .and_then(|profiles| profiles.first())
            .and_then(|profile| profile["validation_status"].as_str()),
        Some("ready")
    );

    let profiles = profiles::all_profiles().unwrap();
    let label = profiles::deploy_prompt_label(
        "qwen3_8b_awq_32k_g5",
        profiles.get("qwen3_8b_awq_32k_g5").unwrap(),
    );
    assert!(label.starts_with("qwen3_8b_awq_32k_g5 —"));
    assert!(label.contains("g5.2xlarge"));
    assert!(label.contains("coding-simple"));
    assert!(label.contains("text"));
    assert!(label.contains("32K"));
    assert!(!label.contains("spot"));
    assert!(!label.contains("on_demand"));

    let gemma_label =
        profiles::deploy_prompt_label("gemma3_12b_128k", profiles.get("gemma3_12b_128k").unwrap());
    assert!(gemma_label.contains("research-general"));
    assert!(gemma_label.contains("multimodal"));

    let inf2_label = profiles::deploy_prompt_label(
        "qwen3_8b_inf2_4k",
        profiles.get("qwen3_8b_inf2_4k").unwrap(),
    );
    assert!(inf2_label.contains("inf2.xlarge"));
    assert!(inf2_label.contains("coding-simple"));
    assert!(inf2_label.contains("text"));
    assert!(inf2_label.contains("4K"));
}

#[test]
fn cost_modes_map_to_terraform_values() {
    assert_eq!(CostMode::from_str("spot").unwrap(), CostMode::Economy);
    assert_eq!(
        CostMode::Economy.terraform_values()["enable_warm_pool"],
        false
    );
    assert_eq!(
        CostMode::Economy.terraform_values()["asg_desired_capacity"],
        1
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
async fn aws_backend_requires_validation_acknowledgement_and_hidden_opt_in() {
    let backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_32k_g5".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    let vars = backend.render_deploy_vars().await.unwrap();
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B-AWQ");
    assert_eq!(vars["max_model_len"], 32768);
    assert_eq!(vars["quantization"], "awq");
    assert_eq!(vars["language_model_only"], true);
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--quantization"));

    let mut hidden = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_32k".to_string()),
            Some("cuda".to_string()),
            Some("g6e.xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    hidden.config.allow_hidden_profiles = true;
    let err = hidden.render_deploy_vars().await.unwrap_err();
    assert!(err.to_string().contains("--acknowledge-unvalidated"));
    hidden.config.acknowledge_unvalidated = true;
    let vars = hidden.render_deploy_vars().await.unwrap();
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B-AWQ");
}

#[tokio::test]
async fn aws_backend_renders_cuda_and_rope_scaling_tfvars() {
    let mut backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_128k".to_string()),
            Some("cuda".to_string()),
            Some("g6e.2xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    backend.config.acknowledge_unvalidated = true;
    let vars = backend.render_deploy_vars().await.unwrap();
    assert_eq!(vars["accelerator"], "cuda");
    assert_eq!(vars["runtime_pack"], "cuda-vllm");
    assert_eq!(vars["enable_warm_pool"], false);
    assert_eq!(vars["enable_idle_scale_down"], true);
    assert_eq!(vars["use_spot_instances"], true);
    assert_eq!(vars["desired_capacity_on_wake"], 1);
    assert_eq!(vars["desired_capacity_on_sleep"], 0);
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B-AWQ");
    assert_eq!(vars["max_model_len"], 131072);
    assert_eq!(vars["quantization"], "awq");
    assert!(vars["rope_scaling_json"]
        .as_str()
        .unwrap()
        .contains("\"rope_type\":\"yarn\""));
    assert_eq!(vars["language_model_only"], true);
    assert_eq!(vars["reasoning_parser"], "qwen3");
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--rope-scaling"));

    let qwen3_g5 = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_32k_g5".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    let mut qwen3_g5 = qwen3_g5;
    qwen3_g5.config.acknowledge_unvalidated = true;
    let vars = qwen3_g5.render_deploy_vars().await.unwrap();
    assert_eq!(vars["instance_type"], "g5.2xlarge");
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B-AWQ");

    let qwen3_inf2 = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_inf2_4k".to_string()),
            Some("inf2".to_string()),
            Some("inf2.xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    let vars = qwen3_inf2.render_deploy_vars().await.unwrap();
    assert_eq!(vars["instance_type"], "inf2.xlarge");
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B");
    assert_eq!(vars["runtime_pack"], "inf2-neuron");
    assert_eq!(vars["max_model_len"], 4096);
    assert_eq!(vars["max_num_seqs"], 8);
    assert_eq!(vars["block_size"], 32);
    assert_eq!(vars["num_gpu_blocks_override"], 8);
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--device neuron"));
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--tensor-parallel-size 2"));
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--max-num-seqs 8"));
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--block-size 32"));
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--num-gpu-blocks-override 8"));
    assert!(vars["vllm_command"]
        .as_str()
        .unwrap()
        .contains("--no-enable-prefix-caching"));
}

#[tokio::test]
async fn aws_backend_renders_direct_deploy_profile_region_and_ami() {
    let mut backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_32k_g5".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    backend.config.acknowledge_unvalidated = true;
    backend.config.profile = Some("emberlane".to_string());
    backend.config.region = "us-west-2".to_string();
    backend.config.ami_id = "ami-1234567890abcdef0".to_string();
    let vars = backend.render_deploy_vars().await.unwrap();
    assert_eq!(vars["aws_profile"], "emberlane");
    assert_eq!(vars["aws_region"], "us-west-2");
    assert_eq!(vars["ami_id"], "ami-1234567890abcdef0");
    assert_eq!(vars["enable_warm_pool"], false);
    assert_eq!(vars["enable_idle_scale_down"], true);
    assert_eq!(vars["use_spot_instances"], true);
    assert_eq!(vars["desired_capacity_on_sleep"], 0);
    assert_eq!(vars["max_model_len"], 32768);
    assert_eq!(vars["language_model_only"], true);
    assert_eq!(vars["reasoning_parser"], "qwen3");

    let mut always_on = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_32k_g5".to_string()),
            Some("cuda".to_string()),
            Some("g5.2xlarge".to_string()),
            Some("always-on".to_string()),
            None,
        )
        .unwrap();
    always_on.config.acknowledge_unvalidated = true;
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
            Some("llama32_1b_inf2_tight_memory".to_string()),
            Some("inf2".to_string()),
            Some("inf2.xlarge".to_string()),
            Some("always-on".to_string()),
            None,
        )
        .unwrap();
    let doctor = backend.doctor().await.unwrap();
    assert!(doctor["warnings"].to_string().contains("experimental"));
    assert!(doctor["warnings"].to_string().contains("hidden"));
    assert_eq!(doctor["capacity"]["skipped"], true);
    let cost = backend.cost_report().await.unwrap();
    assert_eq!(cost["pricing_configured"], true);
    assert_eq!(cost["savings_claimed"], false);
}

#[test]
fn aws_init_config_text_is_cuda_first() {
    let text = AwsBackend::default_config_text().unwrap();
    assert!(text.contains("accelerator = \"cuda\""));
    assert!(text.contains("instance_type = \"g5.2xlarge\""));
    assert!(text.contains("model_profile = \"qwen3_8b_awq_32k_g5\""));
    assert!(text.contains("mode = \"economy\""));
    assert!(text.contains("max_model_len = 32768"));
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
    assert!(readme.contains("AWS Quickstart"));
    assert!(readme.contains("AWS Terraform deployment"));
    assert!(readme.contains("OpenAI-compatible chat endpoints"));
    assert!(readme.contains("Ollama"));
    assert!(readme.contains("vLLM CUDA"));
    assert!(readme.contains("Qwen3-8B-AWQ"));
    assert!(readme.to_lowercase().contains("economy"));
    assert!(readme.to_lowercase().contains("balanced"));
    assert!(readme.contains("Inf2"));
    assert!(!readme.contains("llama32_1b_inf2_economy"));
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
        "quantization",
        "rope_scaling_json",
        "gpu_memory_utilization",
        "enforce_eager",
        "max_num_seqs",
        "block_size",
        "num_gpu_blocks_override",
        "vllm_extra_args",
        "vllm_command",
        "visibility",
        "validation_status",
        "validated",
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
