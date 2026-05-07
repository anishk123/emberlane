use crate::error::EmberlaneError;

#[allow(dead_code)]
pub fn not_implemented(provider: &str) -> EmberlaneError {
    let label = match provider.to_ascii_lowercase().as_str() {
        "gcp" => "GCP".to_string(),
        "azure" => "Azure".to_string(),
        other => other.to_string(),
    };
    EmberlaneError::ProviderNotImplemented(format!(
        "{} backend is not implemented yet. AWS is the first supported backend.",
        label
    ))
}
