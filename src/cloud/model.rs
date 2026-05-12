use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, path::PathBuf, str::FromStr};

use crate::error::EmberlaneError;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn terraform_dir() -> PathBuf {
    repo_root().join("infra/terraform")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudProvider {
    Aws,
    Gcp,
    Azure,
}

impl fmt::Display for CloudProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudProvider::Aws => write!(f, "aws"),
            CloudProvider::Gcp => write!(f, "gcp"),
            CloudProvider::Azure => write!(f, "azure"),
        }
    }
}

impl FromStr for CloudProvider {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "aws" => Ok(Self::Aws),
            "gcp" => Ok(Self::Gcp),
            "azure" => Ok(Self::Azure),
            other => Err(format!("cloud provider '{other}' is not supported")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accelerator {
    Cuda,
    Inf2,
}

impl fmt::Display for Accelerator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Accelerator::Cuda => write!(f, "cuda"),
            Accelerator::Inf2 => write!(f, "inf2"),
        }
    }
}

impl FromStr for Accelerator {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cuda" => Ok(Self::Cuda),
            "inf2" => Ok(Self::Inf2),
            other => Err(format!("accelerator '{other}' is not supported")),
        }
    }
}

impl Accelerator {
    pub fn runtime_pack(self) -> &'static str {
        match self {
            Accelerator::Cuda => "cuda-vllm",
            Accelerator::Inf2 => "inf2-neuron",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudDeployConfig {
    pub provider: CloudProvider,
    pub region: String,
    pub profile: Option<String>,
    pub environment: String,
    pub accelerator: Accelerator,
    pub instance_type: String,
    pub model_profile: String,
    pub mode: crate::cloud::modes::CostMode,
    pub terraform_dir: PathBuf,
    pub api_key: Option<String>,
    pub ami_id: String,
    pub use_baked_ami: bool,
    pub public_alb: bool,
    pub hf_token: Option<String>,
    pub acknowledge_unvalidated: bool,
    pub allow_hidden_profiles: bool,
}

impl Default for CloudDeployConfig {
    fn default() -> Self {
        Self {
            provider: CloudProvider::Aws,
            region: "us-west-2".to_string(),
            profile: None,
            environment: "dev".to_string(),
            accelerator: Accelerator::Inf2,
            instance_type: "inf2.xlarge".to_string(),
            model_profile: "qwen3_4b_inf2_4k".to_string(),
            mode: crate::cloud::modes::CostMode::Economy,
            terraform_dir: terraform_dir(),
            api_key: Some("dev-secret".to_string()),
            ami_id: String::new(),
            use_baked_ami: false,
            public_alb: true,
            hf_token: None,
            acknowledge_unvalidated: false,
            allow_hidden_profiles: false,
        }
    }
}

#[async_trait]
pub trait CloudBackend {
    async fn init_config(&self, force: bool) -> Result<Value, EmberlaneError>;
    async fn doctor(&self) -> Result<Value, EmberlaneError>;
    async fn render_deploy_vars(&self) -> Result<Value, EmberlaneError>;
    async fn deploy(&self, auto_approve: bool, plan_only: bool) -> Result<Value, EmberlaneError>;
    async fn destroy(&self, auto_approve: bool) -> Result<Value, EmberlaneError>;
    async fn status(&self) -> Result<Value, EmberlaneError>;
    async fn chat(&self, message: &str) -> Result<Value, EmberlaneError>;
    async fn benchmark(&self) -> Result<Value, EmberlaneError>;
    async fn cost_report(&self) -> Result<Value, EmberlaneError>;
}
