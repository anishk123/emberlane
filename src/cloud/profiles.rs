use crate::error::EmberlaneError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub display_name: String,
    pub model_id: String,
    pub default_accelerator: String,
    pub recommended_instance: String,
    pub runtime: String,
    pub status: String,
    #[serde(default)]
    pub language_model_only: bool,
    #[serde(default)]
    pub reasoning_parser: Option<String>,
    pub max_model_len: u64,
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

pub fn rows() -> Result<Vec<serde_json::Value>, EmberlaneError> {
    Ok(all_profiles()?
        .into_iter()
        .map(|(name, p)| {
            serde_json::json!({
                "profile": name,
                "display_name": p.display_name,
                "accelerator": p.default_accelerator,
                "recommended_instance": p.recommended_instance,
                "runtime": p.runtime,
                "status": p.status,
                "language_model_only": p.language_model_only,
                "reasoning_parser": p.reasoning_parser,
                "selection_hint": if name.ends_with("_economy") {
                    "tight-memory profile; unrelated to AWS cost mode"
                } else {
                    ""
                },
                "max_model_len": p.max_model_len
            })
        })
        .collect())
}
