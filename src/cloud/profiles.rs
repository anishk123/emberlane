use crate::error::EmberlaneError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RopeScalingConfig {
    pub rope_type: String,
    pub factor: f64,
    pub original_max_position_embeddings: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub display_name: String,
    pub model_id: String,
    pub default_accelerator: String,
    pub recommended_instance: String,
    pub runtime: String,
    pub status: String,
    #[serde(default)]
    pub quantization: Option<String>,
    #[serde(default)]
    pub lower_cost_instance: Option<String>,
    #[serde(default)]
    pub safe_instance: Option<String>,
    #[serde(default)]
    pub default_mode: Option<String>,
    #[serde(default)]
    pub default_pricing: Option<String>,
    #[serde(default)]
    pub balanced_pricing: Option<String>,
    #[serde(default)]
    pub serving_modality: Option<String>,
    #[serde(default)]
    pub task_group: Option<String>,
    #[serde(default)]
    pub instance_group: Option<String>,
    #[serde(default)]
    pub visibility: Option<String>,
    #[serde(default)]
    pub validation_status: Option<String>,
    #[serde(default)]
    pub validated: bool,
    #[serde(default)]
    pub require_user_acknowledgement_if_unvalidated: bool,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub rope_scaling: Option<RopeScalingConfig>,
    #[serde(default)]
    pub language_model_only: bool,
    #[serde(default)]
    pub reasoning_parser: Option<String>,
    #[serde(default)]
    pub tool_call_parser: Option<String>,
    #[serde(default)]
    pub gpu_memory_utilization: Option<f64>,
    #[serde(default)]
    pub enforce_eager: Option<bool>,
    #[serde(default)]
    pub max_num_seqs: Option<u64>,
    #[serde(default)]
    pub block_size: Option<u64>,
    #[serde(default)]
    pub num_gpu_blocks_override: Option<u64>,
    #[serde(default)]
    pub vllm_extra_args: Vec<String>,
    #[serde(default)]
    pub fallback_instances: Vec<String>,
    pub max_model_len: u64,
    #[serde(default)]
    pub use_case: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ProfilesFile {
    models: BTreeMap<String, ModelProfile>,
}

pub fn all_profiles() -> Result<BTreeMap<String, ModelProfile>, EmberlaneError> {
    let parsed: ProfilesFile =
        toml::from_str(include_str!("../../profiles/models.toml")).map_err(|err| {
            EmberlaneError::Internal(format!("failed to parse model profiles: {err}"))
        })?;
    Ok(parsed.models)
}

pub fn profile(name: &str) -> Result<ModelProfile, EmberlaneError> {
    all_profiles()?
        .remove(name)
        .ok_or_else(|| EmberlaneError::InvalidRequest(format!("unknown model profile: {name}")))
}

fn profile_is_visible(profile: &ModelProfile) -> bool {
    !matches!(
        profile.visibility.as_deref().unwrap_or("hidden"),
        "hidden" | "legacy" | "labs"
    )
}

fn profile_visibility(profile: &ModelProfile) -> &str {
    profile.visibility.as_deref().unwrap_or("hidden")
}

fn task_group_rank(task_group: &str) -> u8 {
    match task_group {
        "Coding - Simple" => 0,
        "Recommended — Coding / Simple Agents / Research" => 0,
        "Recommended — Simple Agents / Coding / Research" => 0,
        "Recommended — Budget / Coding / Research" => 0,
        "Coding - Hard" => 1,
        "Hard Agentic / Reasoning" => 1,
        "Reasoning / Hard Agentic" => 1,
        "Research - Deep" => 2,
        "Deep Research / Large Context" => 2,
        "Deep Research / Long Context" => 2,
        "Research - General" => 3,
        "General / Research Alternative" => 3,
        "General Research / Multimodal" => 3,
        "Multimodal" => 4,
        "Hidden / Legacy" => 5,
        _ => 5,
    }
}

fn display_validation_status(profile: &ModelProfile) -> String {
    if profile_visibility(profile) == "hidden" {
        profile
            .validation_status
            .clone()
            .unwrap_or_else(|| "experimental".to_string())
    } else {
        "ready".to_string()
    }
}



fn task_fit_label(profile: &ModelProfile) -> String {
    let task_group = profile.task_group.as_deref().unwrap_or("Hidden / Legacy");
    if task_group.contains("Simple")
        || profile
            .use_case
            .iter()
            .any(|v| v == "quick" || v == "budget" || v == "coding")
    {
        "coding-simple".to_string()
    } else if task_group.contains("Hard")
        || profile
            .use_case
            .iter()
            .any(|v| v == "hard_agentic" || v == "reasoning")
    {
        "coding-hard".to_string()
    } else if task_group.contains("Deep")
        || profile
            .use_case
            .iter()
            .any(|v| v == "deep_research" || v == "large_context")
    {
        "research-deep".to_string()
    } else if task_group.contains("General")
        || profile
            .use_case
            .iter()
            .any(|v| v == "general" || v == "research")
    {
        "research-general".to_string()
    } else {
        "coding-simple".to_string()
    }
}

fn kind_label(profile: &ModelProfile) -> &str {
    match profile.serving_modality.as_deref().unwrap_or("text") {
        "multimodal" => "multimodal",
        _ => "text",
    }
}

pub fn menu_sort_key(name: &str, profile: &ModelProfile) -> (u8, String, String) {
    (
        task_group_rank(profile.task_group.as_deref().unwrap_or("Hidden / Legacy")),
        profile
            .instance_group
            .clone()
            .unwrap_or_else(|| profile.recommended_instance.clone()),
        name.to_string(),
    )
}

pub fn deploy_prompt_label(name: &str, profile: &ModelProfile) -> String {
    let quant = profile.quantization.as_deref().unwrap_or("");
    let task_fit = task_fit_label(profile);
    let kind = kind_label(profile);

    let mut tags = vec![task_fit];
    if profile.use_case.iter().any(|u| u == "reasoning") {
        tags.push("reasoning".to_string());
    }
    tags.push(profile.recommended_instance.clone());
    tags.push(kind.to_string());
    if !quant.is_empty() {
        tags.push(quant.to_string());
    }

    format!(
        "{} — {} [{}]",
        name,
        profile.display_name,
        tags.join(", ")
    )
}

#[allow(dead_code)]
pub fn public_visibility(profile: &ModelProfile) -> bool {
    matches!(
        profile.visibility.as_deref().unwrap_or("hidden"),
        "recommended" | "advanced" | "optional"
    )
}

#[allow(dead_code)]
pub fn rows() -> Result<Vec<serde_json::Value>, EmberlaneError> {
    Ok(all_profiles()?
        .into_iter()
        .filter(|(_, p)| profile_is_visible(p))
        .map(|(name, p)| {
            let validation_status = p
                .validation_status
                .clone()
                .unwrap_or_else(|| "needs_emberlane_validation".to_string());
            let visibility = profile_visibility(&p).to_string();
            serde_json::json!({
                "profile": name,
                "display_name": p.display_name,
                "task_group": p.task_group.unwrap_or_default(),
                "instance_group": p.instance_group.unwrap_or_default(),
                "serving_modality": p.serving_modality.clone().unwrap_or_else(|| "text".to_string()),
                "accelerator": p.default_accelerator,
                "recommended_instance": p.recommended_instance,
                "runtime": p.runtime,
                "quantization": p.quantization,
                "validation_status": validation_status,
                "validated": p.validated,
                "language_model_only": p.language_model_only,
                "reasoning_parser": p.reasoning_parser,
                "fallback_instances": p.fallback_instances,
                "selection_hint": if visibility == "hidden" {
                    "hidden"
                } else if p.serving_modality.as_deref() == Some("multimodal") {
                    "multimodal"
                } else if p.quantization.is_some() {
                    "quantized"
                } else {
                    ""
                },
                "max_model_len": p.max_model_len
            })
        })
        .collect())
}

pub fn menu_sections(show_hidden: bool) -> Result<Vec<serde_json::Value>, EmberlaneError> {
    let mut groups: BTreeMap<String, Vec<(String, ModelProfile)>> = BTreeMap::new();
    for (name, profile) in all_profiles()? {
        if !show_hidden && !profile_is_visible(&profile) {
            continue;
        }
        let group = profile
            .task_group
            .clone()
            .unwrap_or_else(|| "Hidden / Legacy".to_string());
        groups.entry(group).or_default().push((name, profile));
    }

    let mut sections = groups
        .into_iter()
        .collect::<Vec<(String, Vec<(String, ModelProfile)>)>>();
    sections.sort_by_key(|(task_group, _)| task_group_rank(task_group));

    let mut ordered_sections = Vec::new();
    for (task_group, mut items) in sections {
        items.sort_by(|(name_a, a), (name_b, b)| {
            menu_sort_key(name_a, a).cmp(&menu_sort_key(name_b, b))
        });
        let profiles = items
            .into_iter()
            .map(|(name, p)| {
                let validation_status = display_validation_status(&p);
                let caveat = p.note.clone().unwrap_or_default();
                serde_json::json!({
                    "profile": name,
                    "display_name": p.display_name,
                    "model": p.model_id,
                    "context": p.max_model_len,
                    "runtime": p.runtime,
                    "quantization": p.quantization,
                    "recommended_instance": p.recommended_instance,
                    "lower_cost_instance": p.lower_cost_instance,
                    "safe_instance": p.safe_instance,
                    "economy_price": p.default_pricing,
                    "balanced_price": p.balanced_pricing,
                    "validation_status": validation_status,
                    "validated": p.validated,
                    "visibility": profile_visibility(&p),
                    "caveat": caveat,
                })
            })
            .collect::<Vec<_>>();
        ordered_sections.push(serde_json::json!({
            "task_group": task_group,
            "profiles": profiles
        }));
    }

    Ok(ordered_sections)
}

#[allow(dead_code)]
pub fn model_selection_rows(show_hidden: bool) -> Result<Vec<serde_json::Value>, EmberlaneError> {
    Ok(all_profiles()?
        .into_iter()
        .filter(|(_, p)| show_hidden || profile_is_visible(p))
        .map(|(name, p)| {
            let visibility = profile_visibility(&p).to_string();
            let validation_status = display_validation_status(&p);
            serde_json::json!({
                "profile": name,
                "display_name": p.display_name,
                "task_group": p.task_group.unwrap_or_default(),
                "instance_group": p.instance_group.clone().unwrap_or_default(),
                "serving_modality": p.serving_modality.clone().unwrap_or_else(|| "text".to_string()),
                "accelerator": p.default_accelerator,
                "recommended_instance": p.recommended_instance,
                "runtime": p.runtime,
                "status": p.status,
                "quantization": p.quantization,
                "validation_status": validation_status,
                "validated": p.validated,
                "language_model_only": p.language_model_only,
                "reasoning_parser": p.reasoning_parser,
                "fallback_instances": p.fallback_instances,
                "selection_hint": if visibility == "hidden" {
                    if name.contains("quantized") {
                        "quantized"
                    } else if p.serving_modality.as_deref() == Some("multimodal") {
                        "multimodal"
                    } else {
                        "hidden"
                    }
                } else {
                    ""
                },
                "max_model_len": p.max_model_len
            })
        })
        .collect())
}
