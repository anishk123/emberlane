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

    let qwen35_2b = profiles.get("qwen35_2b").unwrap();
    assert_eq!(qwen35_2b.default_accelerator, "cuda");
    assert_eq!(qwen35_2b.recommended_instance, "g5.2xlarge");
    assert_eq!(qwen35_2b.status, "recommended");
    assert_eq!(qwen35_2b.model_id, "Qwen/Qwen3.5-2B");
    assert_eq!(qwen35_2b.quantization.as_deref(), None);
    assert_eq!(qwen35_2b.default_mode.as_deref(), Some("economy"));
    assert_eq!(qwen35_2b.default_pricing.as_deref(), Some("spot"));
    assert_eq!(qwen35_2b.balanced_pricing.as_deref(), Some("on_demand"));
    assert_eq!(qwen35_2b.visibility.as_deref(), Some("recommended"));
    assert_eq!(
        qwen35_2b.validation_status.as_deref(),
        Some("needs_aws_validation")
    );
    assert!(!qwen35_2b.require_user_acknowledgement_if_unvalidated);
    assert!(qwen35_2b.language_model_only);
    assert_eq!(qwen35_2b.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(qwen35_2b.serving_modality.as_deref(), Some("multimodal"));
    assert_eq!(qwen35_2b.max_model_len, 32768);
    assert!(qwen35_2b.use_case.contains(&"single_agent".to_string()));

    let qwen35_2b_awq = profiles.get("qwen35_2b_awq").unwrap();
    assert_eq!(qwen35_2b_awq.default_accelerator, "cuda");
    assert_eq!(qwen35_2b_awq.recommended_instance, "g5.2xlarge");
    assert_eq!(qwen35_2b_awq.status, "advanced");
    assert_eq!(qwen35_2b_awq.model_id, "cyankiwi/Qwen3.5-2B-AWQ-4bit");
    assert_eq!(qwen35_2b_awq.quantization.as_deref(), Some("awq"));
    assert_eq!(qwen35_2b_awq.default_mode.as_deref(), Some("economy"));
    assert_eq!(qwen35_2b_awq.default_pricing.as_deref(), Some("spot"));
    assert_eq!(qwen35_2b_awq.balanced_pricing.as_deref(), Some("on_demand"));
    assert_eq!(qwen35_2b_awq.visibility.as_deref(), Some("advanced"));
    assert_eq!(
        qwen35_2b_awq.validation_status.as_deref(),
        Some("needs_aws_validation")
    );
    assert!(!qwen35_2b_awq.require_user_acknowledgement_if_unvalidated);
    assert!(qwen35_2b_awq.language_model_only);
    assert_eq!(qwen35_2b_awq.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(
        qwen35_2b_awq.serving_modality.as_deref(),
        Some("multimodal")
    );
    assert_eq!(qwen35_2b_awq.max_model_len, 32768);
    assert!(qwen35_2b_awq.use_case.contains(&"multimodal".to_string()));

    let qwen35_9b = profiles.get("qwen35_9b").unwrap();
    assert_eq!(qwen35_9b.default_accelerator, "cuda");
    assert_eq!(qwen35_9b.recommended_instance, "g6e.2xlarge");
    assert_eq!(qwen35_9b.status, "advanced");
    assert_eq!(qwen35_9b.model_id, "Qwen/Qwen3.5-9B");
    assert_eq!(qwen35_9b.quantization.as_deref(), None);
    assert_eq!(qwen35_9b.default_mode.as_deref(), Some("economy"));
    assert_eq!(qwen35_9b.default_pricing.as_deref(), Some("spot"));
    assert_eq!(qwen35_9b.balanced_pricing.as_deref(), Some("on_demand"));
    assert_eq!(qwen35_9b.visibility.as_deref(), Some("advanced"));
    assert_eq!(
        qwen35_9b.validation_status.as_deref(),
        Some("needs_aws_validation")
    );
    assert!(!qwen35_9b.require_user_acknowledgement_if_unvalidated);
    assert!(qwen35_9b.language_model_only);
    assert_eq!(qwen35_9b.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(qwen35_9b.serving_modality.as_deref(), Some("multimodal"));
    assert_eq!(qwen35_9b.max_model_len, 32768);
    assert_eq!(qwen35_9b.safe_instance.as_deref(), Some("g6e.4xlarge"));

    let qwen35_9b_awq = profiles.get("qwen35_9b_awq").unwrap();
    assert_eq!(qwen35_9b_awq.default_accelerator, "cuda");
    assert_eq!(qwen35_9b_awq.recommended_instance, "g6e.2xlarge");
    assert_eq!(qwen35_9b_awq.status, "advanced");
    assert_eq!(qwen35_9b_awq.model_id, "QuantTrio/Qwen3.5-9B-AWQ");
    assert_eq!(qwen35_9b_awq.quantization.as_deref(), Some("awq"));
    assert_eq!(qwen35_9b_awq.default_mode.as_deref(), Some("economy"));
    assert_eq!(qwen35_9b_awq.default_pricing.as_deref(), Some("spot"));
    assert_eq!(qwen35_9b_awq.balanced_pricing.as_deref(), Some("on_demand"));
    assert_eq!(qwen35_9b_awq.visibility.as_deref(), Some("advanced"));
    assert_eq!(
        qwen35_9b_awq.validation_status.as_deref(),
        Some("needs_aws_validation")
    );
    assert!(!qwen35_9b_awq.require_user_acknowledgement_if_unvalidated);
    assert!(qwen35_9b_awq.language_model_only);
    assert_eq!(qwen35_9b_awq.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(
        qwen35_9b_awq.serving_modality.as_deref(),
        Some("multimodal")
    );
    assert_eq!(qwen35_9b_awq.max_model_len, 32768);
    assert_eq!(qwen35_9b_awq.safe_instance.as_deref(), Some("g6e.4xlarge"));

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
        Some("needs_aws_validation")
    );
    assert!(!qwen.require_user_acknowledgement_if_unvalidated);
    assert!(qwen.language_model_only);
    assert_eq!(qwen.reasoning_parser.as_deref(), Some("qwen3"));
    assert_eq!(qwen.serving_modality.as_deref(), Some("text"));
    assert_eq!(qwen.max_model_len, 32768);

    let qwen_g6 = profiles.get("qwen3_8b_awq_32k").unwrap();
    assert_eq!(qwen_g6.visibility.as_deref(), Some("recommended"));
    assert_eq!(qwen_g6.status, "recommended");
    assert_eq!(qwen_g6.recommended_instance, "g6e.xlarge");
    assert!(!qwen_g6.validated);
    assert_eq!(
        qwen_g6.validation_status.as_deref(),
        Some("needs_aws_validation")
    );
    assert_eq!(qwen_g6.serving_modality.as_deref(), Some("text"));
    assert_eq!(
        qwen_g6.task_group.as_deref(),
        Some("Qwen3 — safer coding / research")
    );
    let qwen_g6_label = profiles::deploy_prompt_label("qwen3_8b_awq_32k", qwen_g6);
    assert!(qwen_g6_label.contains("coding"));
    assert!(qwen_g6_label.contains("research"));
    assert!(qwen_g6_label.contains("simple agent"));
    assert!(qwen_g6_label.contains("general"));
    assert!(!qwen_g6_label.contains("Simple coding"));

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
        Some(
            "Use when research needs multimodal capability. May require Hugging Face access/license acceptance."
        )
    );

    let deepseek = profiles.get("deepseek_r1_distill_qwen14b_64k").unwrap();
    assert_eq!(deepseek.status, "advanced");
    assert_eq!(deepseek.max_model_len, 65536);
    assert_eq!(deepseek.task_group.as_deref(), Some("DeepSeek — reasoning"));
    assert_eq!(deepseek.serving_modality.as_deref(), Some("text"));

    let qwen3_4b = profiles.get("qwen3_4b_inf2_4k").unwrap();
    assert_eq!(qwen3_4b.visibility.as_deref(), Some("recommended"));
    assert_eq!(qwen3_4b.default_accelerator, "inf2");
    assert_eq!(qwen3_4b.recommended_instance, "inf2.xlarge");
    assert_eq!(qwen3_4b.runtime, "vllm-neuron");
    assert_eq!(qwen3_4b.status, "recommended");
    assert_eq!(qwen3_4b.model_id, "Qwen/Qwen3-4B");
    assert_eq!(qwen3_4b.max_model_len, 4096);
    assert_eq!(
        qwen3_4b.task_group.as_deref(),
        Some("Qwen3 — cheapest simple agent")
    );
    assert_eq!(qwen3_4b.serving_modality.as_deref(), Some("text"));

    let qwen3_8b_32k = profiles.get("qwen3_8b_inf2_32k").unwrap();
    assert_eq!(qwen3_8b_32k.visibility.as_deref(), Some("advanced"));
    assert_eq!(qwen3_8b_32k.default_accelerator, "inf2");
    assert_eq!(qwen3_8b_32k.recommended_instance, "inf2.8xlarge");
    assert_eq!(qwen3_8b_32k.runtime, "vllm-neuron");
    assert_eq!(qwen3_8b_32k.status, "advanced");
    assert_eq!(qwen3_8b_32k.model_id, "Qwen/Qwen3-8B");
    assert_eq!(qwen3_8b_32k.max_model_len, 32768);
    assert_eq!(qwen3_8b_32k.safe_instance.as_deref(), Some("inf2.24xlarge"));
    assert_eq!(
        qwen3_8b_32k.task_group.as_deref(),
        Some("Qwen3 — cheaper Inf2 32K test")
    );
    assert_eq!(qwen3_8b_32k.serving_modality.as_deref(), Some("text"));

    let qwen25_coder_inf2 = profiles.get("qwen25_coder_7b_inf2_4k").unwrap();
    assert_eq!(qwen25_coder_inf2.default_accelerator, "inf2");
    assert_eq!(qwen25_coder_inf2.recommended_instance, "inf2.xlarge");
    assert_eq!(qwen25_coder_inf2.runtime, "vllm-neuron");
    assert_eq!(qwen25_coder_inf2.status, "legacy_safe");
    assert_eq!(qwen25_coder_inf2.max_model_len, 4096);
    assert_eq!(
        qwen25_coder_inf2.validation_status.as_deref(),
        Some("official_supported_small_context")
    );
    assert!(qwen25_coder_inf2.validated);
    assert_eq!(qwen25_coder_inf2.serving_modality.as_deref(), Some("text"));
    assert_eq!(qwen25_coder_inf2.visibility.as_deref(), Some("hidden"));
    assert_eq!(qwen25_coder_inf2.max_num_seqs, Some(4));
    assert_eq!(qwen25_coder_inf2.block_size, Some(32));
    assert_eq!(qwen25_coder_inf2.num_gpu_blocks_override, Some(4));
    assert_eq!(
        qwen25_coder_inf2.vllm_extra_args,
        vec![
            "--device".to_string(),
            "neuron".to_string(),
            "--tensor-parallel-size".to_string(),
            "2".to_string(),
            "--max-num-seqs".to_string(),
            "4".to_string(),
            "--block-size".to_string(),
            "32".to_string(),
            "--num-gpu-blocks-override".to_string(),
            "4".to_string(),
            "--no-enable-prefix-caching".to_string()
        ]
    );

    let qwen25_inf2 = profiles.get("qwen25_7b_inf2_4k").unwrap();
    assert_eq!(qwen25_inf2.default_accelerator, "inf2");
    assert_eq!(qwen25_inf2.recommended_instance, "inf2.xlarge");
    assert_eq!(qwen25_inf2.runtime, "vllm-neuron");
    assert_eq!(qwen25_inf2.status, "legacy_safe");
    assert_eq!(qwen25_inf2.max_model_len, 4096);
    assert_eq!(
        qwen25_inf2.validation_status.as_deref(),
        Some("official_small_context")
    );
    assert!(qwen25_inf2.validated);
    assert_eq!(qwen25_inf2.serving_modality.as_deref(), Some("text"));
    assert_eq!(qwen25_inf2.visibility.as_deref(), Some("hidden"));
    assert_eq!(qwen25_inf2.max_num_seqs, Some(4));

    let qwen25_inf2_hard = profiles.get("qwen25_14b_inf2_4k").unwrap();
    assert_eq!(qwen25_inf2_hard.default_accelerator, "inf2");
    assert_eq!(qwen25_inf2_hard.recommended_instance, "inf2.8xlarge");
    assert_eq!(qwen25_inf2_hard.runtime, "vllm-neuron");
    assert_eq!(qwen25_inf2_hard.status, "legacy_safe");
    assert_eq!(qwen25_inf2_hard.max_model_len, 4096);
    assert_eq!(
        qwen25_inf2_hard.safe_instance.as_deref(),
        Some("inf2.24xlarge")
    );
    assert_eq!(
        qwen25_inf2_hard.fallback_instances,
        vec!["inf2.24xlarge".to_string()]
    );
    assert_eq!(qwen25_inf2_hard.max_num_seqs, Some(2));
    assert_eq!(qwen25_inf2_hard.num_gpu_blocks_override, Some(2));
    assert!(qwen25_inf2_hard.validated);
    assert_eq!(qwen25_inf2_hard.visibility.as_deref(), Some("hidden"));

    assert!(!profiles.keys().any(|name| name.ends_with("_economy")));
    assert!(!profiles.keys().any(|name| name.ends_with("_candidate")));
}

#[test]
fn hidden_profiles_do_not_show_in_public_rows() {
    let rows = profiles::rows().unwrap();
    assert!(rows.iter().any(|row| row["profile"] == "qwen35_2b"));
    assert!(rows.iter().any(|row| row["profile"] == "qwen35_9b"));
    assert!(rows.iter().any(|row| row["profile"] == "qwen35_2b_awq"));
    assert!(rows.iter().any(|row| row["profile"] == "qwen35_9b_awq"));
    assert!(rows
        .iter()
        .all(|row| row["profile"] != "qwen35_9b_quantized"));
    assert!(rows.iter().any(|row| row["profile"] == "qwen3_4b_inf2_4k"));
    assert!(rows
        .iter()
        .all(|row| row["profile"] != "qwen25_coder_7b_inf2_4k"));
    assert!(rows.iter().all(|row| row["profile"] != "qwen25_7b_inf2_4k"));
    assert!(rows
        .iter()
        .all(|row| row["profile"] != "qwen25_14b_inf2_4k"));
    assert!(rows
        .iter()
        .any(|row| row["profile"] == "qwen3_8b_awq_32k_g5"));
    assert!(rows.iter().any(|row| row["profile"] == "qwen3_8b_awq_32k"));
    assert!(rows.iter().all(|row| row["visibility"] != "hidden"));
    let sections = profiles::menu_sections(false).unwrap();
    assert!(sections
        .iter()
        .any(|section| section["task_group"] == "Qwen3.5 — simple coding / simple agent"));
    assert!(sections
        .iter()
        .any(|section| section["task_group"] == "Gemma3 — multimodal research"));
}

#[test]
fn public_model_menu_prioritizes_recommended_and_shows_instances_clearly() {
    let sections = profiles::menu_sections(false).unwrap();
    assert_eq!(
        sections
            .first()
            .and_then(|section| section["task_group"].as_str()),
        Some("Qwen3.5 — simple coding / simple agent")
    );
    assert_eq!(
        sections
            .first()
            .and_then(|section| section["profiles"].as_array())
            .and_then(|profiles| profiles.first())
            .and_then(|profile| profile["profile"].as_str()),
        Some("qwen35_2b")
    );
    assert_eq!(
        sections
            .first()
            .and_then(|section| section["profiles"].as_array())
            .and_then(|profiles| profiles.get(1))
            .and_then(|profile| profile["profile"].as_str()),
        Some("qwen35_2b_awq")
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
    let label = profiles::deploy_prompt_label("qwen35_2b", profiles.get("qwen35_2b").unwrap());
    assert!(label.starts_with("qwen35_2b —"));
    assert!(label.contains("single agent"));
    assert!(label.contains("simple coding"));
    assert!(label.contains("simple agent"));
    assert!(label.contains("coding"));
    assert!(label.contains("general"));
    assert!(label.contains("multimodal"));
    assert!(label.contains("text"));
    assert!(label.contains("g5.2xlarge"));
    assert!(!label.contains("awq"));
    assert_eq!(label.matches("multimodal").count(), 1);
    assert!(!label.contains("spot"));
    assert!(!label.contains("on_demand"));

    let qwen35_2b_awq_label =
        profiles::deploy_prompt_label("qwen35_2b_awq", profiles.get("qwen35_2b_awq").unwrap());
    assert!(qwen35_2b_awq_label.contains("awq"));
    assert!(qwen35_2b_awq_label.contains("g5.2xlarge"));

    let gemma_label =
        profiles::deploy_prompt_label("gemma3_12b_128k", profiles.get("gemma3_12b_128k").unwrap());
    assert!(gemma_label.contains("general"));
    assert!(gemma_label.contains("research"));
    assert!(gemma_label.contains("multimodal"));

    let qwen35_9b_label =
        profiles::deploy_prompt_label("qwen35_9b", profiles.get("qwen35_9b").unwrap());
    assert!(qwen35_9b_label.contains("hard coding"));
    assert!(qwen35_9b_label.contains("hard agent"));
    assert!(qwen35_9b_label.contains("reasoning"));
    assert!(qwen35_9b_label.contains("deep research"));
    assert!(qwen35_9b_label.contains("research"));
    assert!(qwen35_9b_label.contains("multimodal"));
    assert!(qwen35_9b_label.contains("text"));
    assert!(qwen35_9b_label.contains("g6e.2xlarge"));
    assert_eq!(qwen35_9b_label.matches("multimodal").count(), 1);

    let qwen35_9b_awq_label =
        profiles::deploy_prompt_label("qwen35_9b_awq", profiles.get("qwen35_9b_awq").unwrap());
    assert!(qwen35_9b_awq_label.contains("awq"));
    assert!(qwen35_9b_awq_label.contains("g6e.2xlarge"));

    let inf2_label = profiles::deploy_prompt_label(
        "qwen3_8b_inf2_32k",
        profiles.get("qwen3_8b_inf2_32k").unwrap(),
    );
    assert!(inf2_label.contains("hard agent"));
    assert!(inf2_label.contains("deep research"));
    assert!(inf2_label.contains("large context"));
    assert!(inf2_label.contains("inf2.8xlarge"));
    assert!(inf2_label.contains("text"));
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

#[test]
fn cost_mode_rows_prioritize_balanced_over_economy() {
    let rows = CostMode::rows();
    assert_eq!(
        rows.first().and_then(|row| row["mode"].as_str()),
        Some("balanced")
    );
    assert_eq!(
        rows.get(1).and_then(|row| row["mode"].as_str()),
        Some("economy")
    );
}

#[test]
fn post_apply_wake_normalizes_already_launching() {
    let result = AwsBackend::normalize_post_apply_wake(serde_json::json!({
        "command": "aws autoscaling set-desired-capacity ...",
        "status": 254,
        "stderr": "An error occurred (ScalingActivityInProgress) when calling the SetDesiredCapacity operation",
        "stdout": ""
    }));
    assert_eq!(result["ok"], true);
    assert_eq!(result["state"], "already_launching");
    assert_eq!(result["raw"]["status"], 254);

    let failed = AwsBackend::normalize_post_apply_wake(serde_json::json!({
        "status": 254,
        "stderr": "AccessDenied",
        "stdout": ""
    }));
    assert_eq!(failed["ok"], false);
    assert_eq!(failed["state"], "wake_request_failed");
}

#[tokio::test]
async fn aws_backend_requires_hidden_opt_in() {
    let optional = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen35_9b_quantized".to_string()),
            Some("inf2".to_string()),
            Some("inf2.xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    let err = optional.render_deploy_vars().await.unwrap_err();
    assert!(err.to_string().contains("hidden"));
}

#[tokio::test]
async fn aws_backend_renders_cuda_and_rope_scaling_tfvars() {
    let backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_128k".to_string()),
            Some("cuda".to_string()),
            Some("g6e.2xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
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
    let command = vars["vllm_command"].as_str().unwrap();
    assert!(command.starts_with("serve Qwen/Qwen3-8B-AWQ "));
    assert!(command.contains("--rope-scaling"));
    assert_eq!(
        vars["fallback_instance_types"],
        serde_json::json!(["g6e.4xlarge"])
    );

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
    let vars = qwen3_g5.render_deploy_vars().await.unwrap();
    assert_eq!(vars["instance_type"], "g5.2xlarge");
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B-AWQ");
    assert_eq!(
        vars["fallback_instance_types"],
        serde_json::json!(["g5.4xlarge", "g6e.xlarge"])
    );
    let command = vars["vllm_command"].as_str().unwrap();
    assert!(command.starts_with("serve Qwen/Qwen3-8B-AWQ "));
    assert!(command.contains("--quantization awq"));

    let qwen3_g6e = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_8b_awq_128k".to_string()),
            Some("cuda".to_string()),
            Some("g6e.2xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    let vars = qwen3_g6e.render_deploy_vars().await.unwrap();
    assert_eq!(vars["instance_type"], "g6e.2xlarge");
    assert_eq!(vars["model_id"], "Qwen/Qwen3-8B-AWQ");
    assert_eq!(vars["runtime_pack"], "cuda-vllm");
    assert_eq!(vars["max_model_len"], 131072);
    assert_eq!(vars["quantization"], "awq");
    assert!(vars["rope_scaling_json"]
        .as_str()
        .unwrap()
        .contains("\"rope_type\":\"yarn\""));
    assert_eq!(vars["language_model_only"], true);
    assert_eq!(vars["reasoning_parser"], "qwen3");
    let command = vars["vllm_command"].as_str().unwrap();
    assert!(command.starts_with("serve Qwen/Qwen3-8B-AWQ "));
    assert!(command.contains("--rope-scaling"));

    let qwen3_inf2 = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml")))
        .unwrap()
        .with_overrides(
            Some("qwen3_4b_inf2_4k".to_string()),
            Some("inf2".to_string()),
            Some("inf2.xlarge".to_string()),
            Some("economy".to_string()),
            None,
        )
        .unwrap();
    let vars = qwen3_inf2.render_deploy_vars().await.unwrap();
    assert_eq!(vars["instance_type"], "inf2.xlarge");
    assert_eq!(vars["model_id"], "Qwen/Qwen3-4B");
    assert_eq!(vars["runtime_pack"], "inf2-neuron");
    assert_eq!(
        vars["fallback_instance_types"],
        serde_json::json!(["inf2.8xlarge"])
    );
    let command = vars["vllm_command"].as_str().unwrap();
    assert!(command.starts_with("serve Qwen/Qwen3-4B "));
    assert!(command.contains("--device neuron"));
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
    assert_eq!(vars["fallback_instance_types"], serde_json::json!([]));
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
            Some("qwen35_9b_quantized".to_string()),
            Some("inf2".to_string()),
            Some("inf2.xlarge".to_string()),
            Some("always-on".to_string()),
            None,
        )
        .unwrap();
    let doctor = backend.doctor().await.unwrap();
    assert!(doctor["warnings"]
        .to_string()
        .contains("Inf2/Neuron is experimental"));
    assert_eq!(doctor["capacity"]["skipped"], true);
    let cost = backend.cost_report().await.unwrap();
    let pricing_configured = cost["pricing_configured"].as_bool().unwrap();
    let message = cost["message"].as_str().unwrap();
    if pricing_configured {
        assert!(message.contains("Pricing cache loaded successfully"));
    } else {
        assert!(message.contains("No pricing file is configured"));
    }
    assert_eq!(cost["savings_claimed"], false);
}

#[test]
fn aws_init_config_text_matches_inf2_first_default() {
    let text = AwsBackend::default_config_text().unwrap();
    assert!(text.contains("accelerator = \"inf2\""));
    assert!(text.contains("instance_type = \"inf2.xlarge\""));
    assert!(text.contains("model_profile = \"qwen3_4b_inf2_4k\""));
    assert!(text.contains("mode = \"economy\""));
    assert!(text.contains("max_model_len = 4096"));
    assert!(text.contains("language_model_only = false"));
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
    assert!(readme.contains("Qwen/Qwen3-4B"));
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
        "fallback_instance_types",
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
