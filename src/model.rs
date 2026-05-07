use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    #[default]
    Local,
    S3,
}

impl std::fmt::Display for StorageBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageBackend::Local => write!(f, "local"),
            StorageBackend::S3 => write!(f, "s3"),
        }
    }
}

impl std::str::FromStr for StorageBackend {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "local" => Ok(Self::Local),
            "s3" => Ok(Self::S3),
            other => Err(format!("storage backend '{other}' is not supported")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    #[default]
    Fast,
    Slow,
}

impl std::fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeMode::Fast => write!(f, "fast"),
            RuntimeMode::Slow => write!(f, "slow"),
        }
    }
}

impl std::str::FromStr for RuntimeMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "fast" => Ok(Self::Fast),
            "slow" => Ok(Self::Slow),
            other => Err(format!("unknown runtime mode: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    LocalProcess,
    StaticHttp,
    Mock,
    Ollama,
    AwsAsg,
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderKind::LocalProcess => write!(f, "local_process"),
            ProviderKind::StaticHttp => write!(f, "static_http"),
            ProviderKind::Mock => write!(f, "mock"),
            ProviderKind::Ollama => write!(f, "ollama"),
            ProviderKind::AwsAsg => write!(f, "aws_asg"),
        }
    }
}

impl std::str::FromStr for ProviderKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "local_process" => Ok(Self::LocalProcess),
            "static_http" => Ok(Self::StaticHttp),
            "mock" => Ok(Self::Mock),
            "ollama" => Ok(Self::Ollama),
            "aws_asg" => Ok(Self::AwsAsg),
            other => Err(format!("provider '{other}' is not supported")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub id: String,
    pub name: String,
    pub provider: ProviderKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub mode: RuntimeMode,
    pub base_url: Option<String>,
    #[serde(default = "default_health_path")]
    pub health_path: String,
    #[serde(default = "default_startup_timeout")]
    pub startup_timeout_secs: u64,
    #[serde(default = "default_fast_wait")]
    pub fast_wait_secs: u64,
    #[serde(default = "default_slow_retry")]
    pub slow_retry_after_secs: u64,
    pub idle_ttl_secs: Option<u64>,
    pub max_concurrency: Option<u32>,
    #[serde(default)]
    pub config: Value,
}

fn default_true() -> bool {
    true
}

fn default_health_path() -> String {
    "/health".to_string()
}

fn default_startup_timeout() -> u64 {
    20
}

fn default_fast_wait() -> u64 {
    10
}

fn default_slow_retry() -> u64 {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStateKind {
    Cold,
    Waking,
    Ready,
    Sleeping,
    Failed,
    Unknown,
}

impl std::fmt::Display for RuntimeStateKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RuntimeStateKind::Cold => "cold",
            RuntimeStateKind::Waking => "waking",
            RuntimeStateKind::Ready => "ready",
            RuntimeStateKind::Sleeping => "sleeping",
            RuntimeStateKind::Failed => "failed",
            RuntimeStateKind::Unknown => "unknown",
        };
        write!(f, "{s}")
    }
}

impl std::str::FromStr for RuntimeStateKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cold" => Ok(Self::Cold),
            "waking" => Ok(Self::Waking),
            "ready" => Ok(Self::Ready),
            "sleeping" => Ok(Self::Sleeping),
            "failed" => Ok(Self::Failed),
            "unknown" => Ok(Self::Unknown),
            other => Err(format!("unknown runtime state: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    pub runtime_id: String,
    pub state: RuntimeStateKind,
    pub last_health_at: Option<DateTime<Utc>>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub last_ready_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub in_flight: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub runtime: RuntimeConfig,
    pub state: RuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub provider_status: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    #[serde(default = "default_post")]
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Value,
}

fn default_post() -> String {
    "POST".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    #[serde(alias = "file_id")]
    pub id: String,
    pub original_name: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stored_path: Option<String>,
    #[serde(default)]
    pub storage_backend: StorageBackend,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_key: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub s3_uri: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: i64,
    pub sha256: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: String,
    pub runtime_id: Option<String>,
    pub event_type: String,
    pub message: Option<String>,
    pub data_json: Option<Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmingResponse {
    pub runtime_id: String,
    pub state: String,
    pub mode: RuntimeMode,
    pub message: String,
    pub retry_after_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub ok: bool,
    pub error: ApiErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorDetails {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub details: Value,
}
