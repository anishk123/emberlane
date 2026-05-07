use crate::error::EmberlaneError;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{self, IsTerminal, Write},
    path::Path,
};

pub fn now() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

pub fn uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[allow(dead_code)]
pub fn redact_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.iter()
                .map(|(k, v)| {
                    if is_secret_key(k) {
                        (
                            k.clone(),
                            serde_json::Value::String("[redacted]".to_string()),
                        )
                    } else {
                        (k.clone(), redact_value(v))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.iter().map(redact_value).collect())
        }
        other => other.clone(),
    }
}

#[allow(dead_code)]
pub fn redact_headers(headers: &HashMap<String, String>) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| {
            if is_secret_key(k) {
                (k.clone(), "[redacted]".to_string())
            } else {
                (k.clone(), v.clone())
            }
        })
        .collect()
}

fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "api_key",
        "access_key",
        "secret",
        "token",
        "password",
        "authorization",
        "presigned",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

pub fn redact_text(text: &str) -> String {
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if [
                "api_key",
                "access_key",
                "secret",
                "token",
                "password",
                "authorization",
                "presigned",
                "x-amz-signature",
            ]
            .iter()
            .any(|needle| lower.contains(needle))
            {
                "[redacted]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn safe_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("upload.bin")
        .replace(['/', '\\', ':'], "_")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn join_url(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    format!("{base}{path}")
}

pub fn prompt_confirm(prompt: &str) -> Result<bool, EmberlaneError> {
    if !io::stdin().is_terminal() {
        return Err(EmberlaneError::InvalidRequest(format!(
            "{prompt} -- re-run with --auto-approve or from an interactive terminal"
        )));
    }
    eprint!("{prompt} [y/N]: ");
    io::stderr().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let answer = input.trim().to_ascii_lowercase();
    Ok(matches!(answer.as_str(), "y" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redacts_secrets() {
        let v = json!({"api_key":"x","nested":{"Authorization":"Bearer y"},"ok":"z"});
        let redacted = redact_value(&v);
        assert_eq!(redacted["api_key"], "[redacted]");
        assert_eq!(redacted["nested"]["Authorization"], "[redacted]");
        assert_eq!(redacted["ok"], "z");
    }
}
