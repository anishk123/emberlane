use super::{
    model::{repo_root, Accelerator, CloudBackend, CloudDeployConfig, CloudProvider},
    modes::CostMode,
    pricing, profiles,
};
use crate::{
    config::{EmberlaneConfig, S3StorageConfig},
    error::EmberlaneError,
    model::StorageBackend,
    util,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs, path::PathBuf, process::Stdio, time::Instant};
use tokio::process::Command;

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        "''".to_string()
    } else if !value
        .chars()
        .any(|ch| ch.is_whitespace() || matches!(ch, '\'' | '"' | '$' | '`' | '\\'))
    {
        value.to_string()
    } else {
        let escaped = value.replace('\'', r"'\''");
        format!("'{}'", escaped)
    }
}

fn aws_config_path() -> PathBuf {
    repo_root().join("aws/emberlane.aws.toml")
}

pub fn terraform_install_help() -> &'static str {
    "terraform was not found; install Terraform >= 1.6.0.\n\nmacOS with Homebrew:\n  brew tap hashicorp/tap\n  brew install hashicorp/tap/terraform\n\nVerify:\n  terraform version\n\nIf you only want to render Terraform variables without applying yet:\n  cargo run -- aws deploy --plan-only\n\nIf you authenticated with an AWS profile, pass it directly or save it first:\n  cargo run -- aws deploy --profile emberlane ...\n  cargo run -- aws init --profile emberlane --force"
}

#[derive(Debug, Serialize, Deserialize)]
struct AwsFile {
    aws: AwsSection,
    deploy: DeploySection,
    terraform: TerraformSection,
    benchmark: BenchmarkSection,
    future: FutureSection,
}

#[derive(Debug, Serialize, Deserialize)]
struct AwsSection {
    region: String,
    profile: String,
    environment: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct DeploySection {
    app_name: String,
    accelerator: String,
    instance_type: String,
    model_profile: String,
    mode: String,
    ami_id: String,
    #[serde(default)]
    max_model_len: u64,
    #[serde(default)]
    language_model_only: bool,
    #[serde(default)]
    reasoning_parser: String,
    use_baked_ami: bool,
    public_alb: bool,
    api_key: String,
    endpoint_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct TerraformSection {
    dir: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BenchmarkSection {
    prompt: String,
    timeout_secs: u64,
    retry_interval_secs: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct FutureSection {
    gcp_enabled: bool,
    azure_enabled: bool,
}

pub struct AwsBackend {
    path: PathBuf,
    pub config: CloudDeployConfig,
    benchmark_prompt: String,
    benchmark_timeout_secs: u64,
    benchmark_retry_interval_secs: u64,
    endpoint_url: Option<String>,
}

impl AwsBackend {
    pub fn load_or_default(path: Option<PathBuf>) -> Result<Self, EmberlaneError> {
        let path = path.unwrap_or_else(aws_config_path);
        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::from_config(path, CloudDeployConfig::default()))
        }
    }

    pub fn load(path: PathBuf) -> Result<Self, EmberlaneError> {
        let text = fs::read_to_string(&path)?;
        let parsed: AwsFile = toml::from_str(&text)
            .map_err(|err| EmberlaneError::InvalidRequest(format!("invalid AWS config: {err}")))?;
        let accelerator = parsed.deploy.accelerator.parse().map_err(|e: String| {
            EmberlaneError::InvalidRequest(format!("invalid deploy.accelerator: {e}"))
        })?;
        let mode = parsed.deploy.mode.parse().map_err(|e: String| {
            EmberlaneError::InvalidRequest(format!("invalid deploy.mode: {e}"))
        })?;
        let terraform_dir = {
            let dir = PathBuf::from(parsed.terraform.dir);
            if dir.is_absolute() {
                dir
            } else {
                repo_root().join(dir)
            }
        };
        let cfg = CloudDeployConfig {
            provider: CloudProvider::Aws,
            region: parsed.aws.region,
            profile: if parsed.aws.profile.trim().is_empty() {
                None
            } else {
                Some(parsed.aws.profile)
            },
            environment: parsed.aws.environment,
            accelerator,
            instance_type: parsed.deploy.instance_type,
            model_profile: parsed.deploy.model_profile,
            mode,
            terraform_dir,
            api_key: if parsed.deploy.api_key.trim().is_empty() {
                None
            } else {
                Some(parsed.deploy.api_key)
            },
            ami_id: parsed.deploy.ami_id,
            use_baked_ami: parsed.deploy.use_baked_ami,
            public_alb: parsed.deploy.public_alb,
            hf_token: None,
            acknowledge_unvalidated: false,
            allow_hidden_profiles: false,
        };
        Ok(Self {
            path,
            config: cfg,
            benchmark_prompt: parsed.benchmark.prompt,
            benchmark_timeout_secs: parsed.benchmark.timeout_secs,
            benchmark_retry_interval_secs: parsed.benchmark.retry_interval_secs,
            endpoint_url: if parsed.deploy.endpoint_url.trim().is_empty() {
                None
            } else {
                Some(parsed.deploy.endpoint_url)
            },
        })
    }

    fn from_config(path: PathBuf, config: CloudDeployConfig) -> Self {
        Self {
            path,
            config,
            benchmark_prompt: "Explain scale-to-zero inference in two sentences.".to_string(),
            benchmark_timeout_secs: 900,
            benchmark_retry_interval_secs: 10,
            endpoint_url: None,
        }
    }

    pub fn with_overrides(
        mut self,
        model: Option<String>,
        accelerator: Option<String>,
        instance: Option<String>,
        mode: Option<String>,
        hf_token: Option<String>,
    ) -> Result<Self, EmberlaneError> {
        if let Some(model) = model {
            self.config.model_profile = model;
            let profile = profiles::profile(&self.config.model_profile)?;
            // If the model changed, we should default to its recommended settings
            // unless the user specifically overrides them in this same call.
            if accelerator.is_none() {
                self.config.accelerator = profile
                    .default_accelerator
                    .parse()
                    .map_err(EmberlaneError::InvalidRequest)?;
            }
            if instance.is_none() {
                self.config.instance_type = profile.recommended_instance;
            }
            if mode.is_none() {
                if let Some(default_mode) = profile.default_mode.as_deref() {
                    self.config.mode = default_mode
                        .parse()
                        .map_err(EmberlaneError::InvalidRequest)?;
                }
            }
        }
        if let Some(acc) = accelerator {
            self.config.accelerator = acc.parse().map_err(EmberlaneError::InvalidRequest)?;
        }
        if let Some(inst) = instance {
            self.config.instance_type = inst;
        }
        if let Some(m) = mode {
            self.config.mode = m.parse().map_err(EmberlaneError::InvalidRequest)?;
        }
        // Fallback for empty config (e.g. if loaded empty default)
        if self.config.instance_type.is_empty() {
            let profile = profiles::profile(&self.config.model_profile)?;
            self.config.instance_type = profile.recommended_instance;
        }
        if let Some(token) = hf_token {
            self.config.hf_token = Some(token);
        }

        Ok(self)
    }

    fn to_file(&self) -> Result<AwsFile, EmberlaneError> {
        let profile = profiles::profile(&self.config.model_profile)?;
        Ok(AwsFile {
            aws: AwsSection {
                region: self.config.region.clone(),
                profile: self.config.profile.clone().unwrap_or_default(),
                environment: self.config.environment.clone(),
            },
            deploy: DeploySection {
                app_name: "emberlane".to_string(),
                accelerator: self.config.accelerator.to_string(),
                instance_type: self.config.instance_type.clone(),
                model_profile: self.config.model_profile.clone(),
                mode: self.config.mode.to_string(),
                ami_id: self.config.ami_id.clone(),
                max_model_len: profile.max_model_len,
                language_model_only: profile.language_model_only,
                reasoning_parser: profile.reasoning_parser.unwrap_or_default(),
                use_baked_ami: self.config.use_baked_ami,
                public_alb: self.config.public_alb,
                api_key: self.config.api_key.clone().unwrap_or_default(),
                endpoint_url: self.endpoint_url.clone().unwrap_or_default(),
            },
            terraform: TerraformSection {
                dir: self.config.terraform_dir.display().to_string(),
            },
            benchmark: BenchmarkSection {
                prompt: self.benchmark_prompt.clone(),
                timeout_secs: self.benchmark_timeout_secs,
                retry_interval_secs: self.benchmark_retry_interval_secs,
            },
            future: FutureSection {
                gcp_enabled: false,
                azure_enabled: false,
            },
        })
    }

    #[allow(dead_code)]
    pub fn default_config_text() -> Result<String, EmberlaneError> {
        let backend = Self::from_config(aws_config_path(), CloudDeployConfig::default());
        toml::to_string_pretty(&backend.to_file()?)
            .map_err(|err| EmberlaneError::Internal(format!("failed to render AWS config: {err}")))
    }

    fn tfvars_path(&self) -> PathBuf {
        self.config.terraform_dir.join("terraform.tfvars.json")
    }

    async fn run_aws_cli(&self, args: &[&str]) -> Result<Value, EmberlaneError> {
        let aws_cli = self.aws_cli();
        let output = Command::new(&aws_cli)
            .args(args)
            .env("AWS_REGION", &self.config.region)
            .env("AWS_DEFAULT_REGION", &self.config.region)
            .envs(
                self.config
                    .profile
                    .as_ref()
                    .map(|profile| [("AWS_PROFILE", profile.as_str())])
                    .into_iter()
                    .flatten(),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|err| {
                EmberlaneError::Internal(format!("failed to run AWS CLI '{}': {err}", aws_cli))
            })?;
        Ok(json!({
            "command": format!("{} {}", aws_cli, args.join(" ")),
            "status": output.status.code().unwrap_or(1),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr)
        }))
    }

    fn aws_cli(&self) -> String {
        self.endpoint_url
            .as_deref()
            .map(|_| "aws".to_string())
            .unwrap_or_else(|| "aws".to_string())
    }

    async fn resolve_ami_id(&self) -> Result<String, EmberlaneError> {
        if !self.config.ami_id.trim().is_empty() {
            return Ok(self.config.ami_id.clone());
        }
        if cfg!(test) || std::env::var("EMBERLANE_SKIP_AMI_LOOKUP").ok().as_deref() == Some("1") {
            return Ok(String::new());
        }

        let ssm_name = match self.config.accelerator {
            Accelerator::Cuda => "/aws/service/deeplearning/ami/x86_64/base-oss-nvidia-driver-gpu-ubuntu-24.04/latest/ami-id",
            Accelerator::Inf2 => "/aws/service/neuron/dlami/base/ubuntu-24.04/latest/image_id",
        };

        let result = self
            .run_aws_cli(&[
                "ssm",
                "get-parameter",
                "--region",
                &self.config.region,
                "--name",
                ssm_name,
                "--query",
                "Parameter.Value",
                "--output",
                "text",
            ])
            .await?;

        if result["status"].as_i64().unwrap_or(1) == 0 {
            let value = result["stdout"].as_str().unwrap_or("").trim().to_string();
            if !value.is_empty() && value != "None" && value != "null" {
                return Ok(value);
            }
        }

        Err(EmberlaneError::ProviderNotConfigured(format!(
            "could not resolve {} AMI via SSM parameter {}. Verify your AWS region and permissions.",
            self.config.accelerator, ssm_name
        )))
    }

    async fn instance_type_offering_report(&self) -> Result<Value, EmberlaneError> {
        if cfg!(test)
            || std::env::var("EMBERLANE_SKIP_CAPACITY_LOOKUP")
                .ok()
                .as_deref()
                == Some("1")
        {
            return Ok(json!({"supported": null, "skipped": true}));
        }

        let instance_type = self.config.instance_type.clone();
        let region = self.config.region.clone();
        let instance_filter = format!("Name=instance-type,Values={instance_type}");
        let location_filter = format!("Name=location,Values={region}");
        let result = self
            .run_aws_cli(&[
                "ec2",
                "describe-instance-type-offerings",
                "--location-type",
                "region",
                "--filters",
                &instance_filter,
                &location_filter,
                "--region",
                &region,
                "--output",
                "json",
            ])
            .await?;

        let status = result["status"].as_i64().unwrap_or(1);
        if status != 0 {
            return Ok(json!({
                "supported": null,
                "status": status,
                "stderr": result["stderr"],
                "stdout": result["stdout"],
            }));
        }

        let offerings = serde_json::from_str::<Value>(result["stdout"].as_str().unwrap_or("{}"))
            .unwrap_or_else(|_| json!({}));
        let supported = offerings
            .get("InstanceTypeOfferings")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(false);

        Ok(json!({
            "supported": supported,
            "instance_type": instance_type,
            "region": region,
            "offerings": offerings.get("InstanceTypeOfferings").cloned().unwrap_or_else(|| json!([]))
        }))
    }

    fn spot_quota_family_for_instance(instance_type: &str) -> Option<&'static str> {
        if instance_type.starts_with("g") {
            Some("All G and VT Spot Instance Requests")
        } else if instance_type.starts_with("inf") {
            Some("All Inf Spot Instance Requests")
        } else {
            None
        }
    }

    async fn instance_vcpu_count(&self) -> Result<Option<u64>, EmberlaneError> {
        let instance_type = self.config.instance_type.clone();
        let region = self.config.region.clone();
        let result = self
            .run_aws_cli(&[
                "ec2",
                "describe-instance-types",
                "--instance-types",
                &instance_type,
                "--query",
                "InstanceTypes[0].VCpuInfo.DefaultVCpus",
                "--output",
                "text",
                "--region",
                &region,
            ])
            .await?;
        if result["status"].as_i64().unwrap_or(1) != 0 {
            return Ok(None);
        }
        let value = result["stdout"].as_str().unwrap_or("").trim();
        if value.is_empty() || value == "None" || value == "null" {
            return Ok(None);
        }
        Ok(value.parse::<u64>().ok())
    }

    async fn spot_quota_value(&self, quota_name: &str) -> Result<Option<f64>, EmberlaneError> {
        let region = self.config.region.clone();
        let query = format!("Quotas[?QuotaName=='{quota_name}'].Value | [0]");
        let result = self
            .run_aws_cli(&[
                "service-quotas",
                "list-service-quotas",
                "--service-code",
                "ec2",
                "--query",
                &query,
                "--output",
                "text",
                "--region",
                &region,
            ])
            .await?;
        if result["status"].as_i64().unwrap_or(1) != 0 {
            return Ok(None);
        }
        let value = result["stdout"].as_str().unwrap_or("").trim();
        if value.is_empty() || value == "None" || value == "null" {
            return Ok(None);
        }
        Ok(value.parse::<f64>().ok())
    }

    fn capacity_fallbacks(&self) -> Vec<String> {
        profiles::profile(&self.config.model_profile)
            .map(|p| {
                let mut fallbacks = vec![p.recommended_instance.clone()];
                if let Some(safe_instance) = p.safe_instance {
                    fallbacks.push(safe_instance);
                }
                if let Some(lower_cost_instance) = p.lower_cost_instance {
                    fallbacks.push(lower_cost_instance);
                }
                fallbacks.extend(p.fallback_instances);
                fallbacks.retain(|value| !value.trim().is_empty());
                let mut seen = BTreeSet::new();
                fallbacks.retain(|value| seen.insert(value.clone()));
                fallbacks
            })
            .unwrap_or_default()
    }

    fn spot_fallback_instance_types(&self) -> Vec<String> {
        if !matches!(self.config.mode, CostMode::Economy) {
            return Vec::new();
        }
        self.capacity_fallbacks()
            .into_iter()
            .filter(|instance| instance != &self.config.instance_type)
            .collect()
    }

    fn profile_visibility(profile: &profiles::ModelProfile) -> &str {
        profile.visibility.as_deref().unwrap_or("hidden")
    }

    fn requires_acknowledgement(profile: &profiles::ModelProfile) -> bool {
        !profile.validated && profile.require_user_acknowledgement_if_unvalidated
    }

    fn build_vllm_command(
        profile: &profiles::ModelProfile,
        accelerator: Accelerator,
    ) -> Result<String, EmberlaneError> {
        let mut args = vec![
            "serve".to_string(),
            profile.model_id.clone(),
            "--max-model-len".to_string(),
            profile.max_model_len.to_string(),
        ];
        if accelerator == Accelerator::Cuda {
            args.push("--safetensors-load-strategy=prefetch".to_string());
        }
        if let Some(quantization) = profile.quantization.as_ref() {
            args.push("--quantization".to_string());
            args.push(quantization.clone());
        }
        if let Some(rope_scaling) = profile.rope_scaling.as_ref() {
            let rope_json = serde_json::to_string(rope_scaling).map_err(|err| {
                EmberlaneError::Internal(format!("failed to render rope scaling: {err}"))
            })?;
            args.push("--rope-scaling".to_string());
            args.push(rope_json);
        }
        if let Some(gpu_memory_utilization) = profile.gpu_memory_utilization {
            args.push("--gpu-memory-utilization".to_string());
            args.push(gpu_memory_utilization.to_string());
        }
        if profile.enforce_eager.unwrap_or(false) {
            args.push("--enforce-eager".to_string());
        }
        if profile.language_model_only {
            args.push("--language-model-only".to_string());
        }
        if let Some(reasoning_parser) = profile.reasoning_parser.as_ref() {
            args.push("--reasoning-parser".to_string());
            args.push(reasoning_parser.clone());
        }
        if let Some(tool_call_parser) = profile.tool_call_parser.as_ref() {
            args.push("--tool-call-parser".to_string());
            args.push(tool_call_parser.clone());
        }
        args.extend(profile.vllm_extra_args.clone());
        args.extend([
            "--host".to_string(),
            "0.0.0.0".to_string(),
            "--port".to_string(),
            "8000".to_string(),
        ]);
        Ok(args
            .into_iter()
            .map(|arg| shell_quote(&arg))
            .collect::<Vec<_>>()
            .join(" "))
    }

    async fn ensure_instance_type_available(&self) -> Result<(), EmberlaneError> {
        let report = self.instance_type_offering_report().await?;
        if report.get("supported").and_then(Value::as_bool) == Some(false) {
            let fallbacks = self.capacity_fallbacks();
            let fallback_text = if fallbacks.is_empty() {
                "no explicit fallback instances are listed for this model profile".to_string()
            } else {
                format!("try one of: {}", fallbacks.join(", "))
            };
            return Err(EmberlaneError::ProviderNotConfigured(format!(
                "AWS does not currently list instance type '{}' in region '{}'. For model profile '{}', {}.",
                self.config.instance_type, self.config.region, self.config.model_profile, fallback_text
            )));
        }
        Ok(())
    }

    async fn ensure_spot_quota_available(&self) -> Result<(), EmberlaneError> {
        if !matches!(self.config.mode, CostMode::Economy) {
            return Ok(());
        }

        let Some(quota_name) = Self::spot_quota_family_for_instance(&self.config.instance_type)
        else {
            return Ok(());
        };

        let Some(vcpus) = self.instance_vcpu_count().await? else {
            return Ok(());
        };

        let Some(quota_value) = self.spot_quota_value(quota_name).await? else {
            return Ok(());
        };

        if quota_value < vcpus as f64 {
            return Err(EmberlaneError::ProviderNotConfigured(format!(
                "Spot quota for '{}' in region '{}' is {:.0} vCPUs, but '{}' needs {} vCPUs. Use --mode balanced for on-demand capacity or request a Spot quota increase in EC2 Service Quotas.",
                quota_name,
                self.config.region,
                quota_value,
                self.config.instance_type,
                vcpus
            )));
        }

        Ok(())
    }

    async fn run_terraform(&self, args: &[&str], stream: bool) -> Result<Value, EmberlaneError> {
        let mut cmd = Command::new("terraform");
        cmd.args(args)
            .current_dir(&self.config.terraform_dir)
            .env("AWS_REGION", &self.config.region)
            .env("AWS_DEFAULT_REGION", &self.config.region)
            .envs(
                self.config
                    .profile
                    .as_ref()
                    .map(|profile| [("AWS_PROFILE", profile.as_str())])
                    .into_iter()
                    .flatten(),
            );
        if stream {
            cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            let status = cmd.status().await.map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    EmberlaneError::ProviderNotConfigured(terraform_install_help().to_string())
                } else {
                    EmberlaneError::Internal(format!("failed to run terraform: {err}"))
                }
            })?;
            Ok(json!({
                "command": format!("terraform {}", args.join(" ")),
                "status": status.code().unwrap_or(1),
                "stdout": "",
                "stderr": ""
            }))
        } else {
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
            let output = cmd.output().await.map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    EmberlaneError::ProviderNotConfigured(terraform_install_help().to_string())
                } else {
                    EmberlaneError::Internal(format!("failed to run terraform: {err}"))
                }
            })?;
            Ok(json!({
                "command": format!("terraform {}", args.join(" ")),
                "status": output.status.code().unwrap_or(1),
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr)
            }))
        }
    }

    async fn endpoint(&self) -> Result<String, EmberlaneError> {
        let url = if let Some(url) = &self.endpoint_url {
            url.clone()
        } else if let Ok(url) = std::env::var("EMBERLANE_AWS_ENDPOINT") {
            url
        } else {
            let output = Command::new("terraform")
                .args(["output", "-raw", "lambda_function_url"])
                .current_dir(&self.config.terraform_dir)
                .output()
                .await
                .map_err(|_| {
                    EmberlaneError::ProviderNotConfigured(
                        "AWS endpoint is not configured. Run terraform apply first.".to_string(),
                    )
                })?;
            if output.status.success() {
                String::from_utf8_lossy(&output.stdout).to_string()
            } else {
                return Err(EmberlaneError::ProviderNotConfigured(
                    "failed to read Terraform lambda_function_url output".to_string(),
                ));
            }
        };

        let cleaned = url
            .trim()
            .trim_matches('"')
            .trim_end_matches('/')
            .to_string();
        if cleaned.is_empty() {
            return Err(EmberlaneError::ProviderNotConfigured(
                "AWS endpoint URL is empty".to_string(),
            ));
        }
        Ok(cleaned)
    }

    pub async fn endpoint_url(&self) -> Result<String, EmberlaneError> {
        self.endpoint().await
    }

    async fn alb_endpoint(&self) -> Result<Option<String>, EmberlaneError> {
        let output = self
            .run_terraform(&["output", "-raw", "alb_url"], false)
            .await?;
        if output["status"].as_i64().unwrap_or(1) != 0 {
            return Ok(None);
        }
        let value = output["stdout"]
            .as_str()
            .unwrap_or("")
            .trim()
            .trim_matches('"')
            .trim_end_matches('/')
            .to_string();
        Ok((!value.is_empty()).then_some(value))
    }

    fn terraform_outputs(stdout: Option<&str>) -> Option<Value> {
        stdout.and_then(|text| serde_json::from_str::<Value>(text).ok())
    }

    fn terraform_output_string(outputs: Option<&Value>, key: &str) -> Option<String> {
        outputs
            .and_then(|value| value.get(key))
            .and_then(|value| value.get("value"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    }

    pub(crate) fn normalize_post_apply_wake(result: Value) -> Value {
        let status = result.get("status").and_then(Value::as_i64).unwrap_or(1);
        let stderr = result.get("stderr").and_then(Value::as_str).unwrap_or("");
        if status == 0 {
            return json!({
                "ok": true,
                "state": "wake_requested",
                "message": "Requested ASG desired capacity after deploy.",
                "raw": result
            });
        }
        if stderr.contains("ScalingActivityInProgress") {
            return json!({
                "ok": true,
                "state": "already_launching",
                "message": "ASG is already launching capacity; no extra wake request is needed.",
                "raw": result
            });
        }
        json!({
            "ok": false,
            "state": "wake_request_failed",
            "message": "Post-apply ASG wake request failed. Check ASG scaling activities for the current capacity state.",
            "raw": result
        })
    }

    async fn reconcile_asg_desired_capacity(
        &self,
        outputs: Option<&Value>,
        vars: &Value,
        apply: &Value,
    ) -> Result<Value, EmberlaneError> {
        if apply["status"].as_i64().unwrap_or(1) != 0 {
            return Ok(json!({
                "skipped": true,
                "reason": "terraform apply did not succeed"
            }));
        }

        let desired = vars
            .get("asg_desired_capacity")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        if desired <= 0 {
            return Ok(json!({
                "skipped": true,
                "reason": "rendered ASG desired capacity is zero"
            }));
        }

        let Some(asg_name) = Self::terraform_output_string(outputs, "asg_name") else {
            return Ok(json!({
                "skipped": true,
                "reason": "terraform output asg_name is unavailable"
            }));
        };

        let result = self
            .run_aws_cli(&[
                "autoscaling",
                "set-desired-capacity",
                "--auto-scaling-group-name",
                &asg_name,
                "--desired-capacity",
                &desired.to_string(),
                "--honor-cooldown",
                "--region",
                &self.config.region,
            ])
            .await?;
        Ok(Self::normalize_post_apply_wake(result))
    }

    async fn post_chat(
        &self,
        endpoint: &str,
        message: &str,
        include_auth: bool,
    ) -> Result<Value, EmberlaneError> {
        let profile = profiles::profile(&self.config.model_profile)?;
        let mut body = json!({
            "model": profile.model_id,
            "messages": [{"role": "user", "content": message}],
            "max_tokens": 512,
            "temperature": 0.2,
            "stream": false
        });
        if profile.reasoning_parser.as_deref() == Some("qwen3") {
            body["chat_template_kwargs"] = json!({"enable_thinking": false});
        }
        let mut req = reqwest::Client::new()
            .post(format!("{endpoint}/v1/chat/completions"))
            .json(&body);
        if include_auth {
            if let Some(api_key) = &self.config.api_key {
                req = req.bearer_auth(api_key).header("x-api-key", api_key);
            }
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let json_body = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok(json!({"status": status, "body": json_body, "endpoint": endpoint}))
    }

    fn should_wake_through_lambda(result: &Value) -> bool {
        matches!(
            result.get("status").and_then(Value::as_u64),
            Some(401 | 403 | 404 | 502 | 503 | 504)
        )
    }

    fn is_lambda_timeout(result: &Value) -> bool {
        result
            .get("body")
            .and_then(|body| body.get("errorType"))
            .and_then(Value::as_str)
            == Some("Sandbox.Timedout")
    }
}

#[async_trait]
impl CloudBackend for AwsBackend {
    async fn init_config(&self, force: bool) -> Result<Value, EmberlaneError> {
        if self.path.exists() && !force {
            return Err(EmberlaneError::InvalidRequest(format!(
                "{} already exists; pass --force to overwrite",
                self.path.display()
            )));
        }
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(&self.to_file()?).map_err(|err| {
            EmberlaneError::Internal(format!("failed to render AWS config: {err}"))
        })?;
        fs::write(&self.path, text)?;
        Ok(json!({"path": self.path, "message": "created AWS deploy config"}))
    }

    async fn doctor(&self) -> Result<Value, EmberlaneError> {
        let profile = profiles::profile(&self.config.model_profile)?;
        let mut warnings = Vec::new();
        if profile.default_accelerator != self.config.accelerator.to_string() {
            warnings.push(format!(
                "model profile '{}' defaults to accelerator '{}', but deploy accelerator is '{}'",
                self.config.model_profile, profile.default_accelerator, self.config.accelerator
            ));
        }
        if profile.recommended_instance != self.config.instance_type {
            warnings.push(format!(
                "model profile '{}' recommends instance '{}', but deploy instance is '{}'",
                self.config.model_profile, profile.recommended_instance, self.config.instance_type
            ));
        }
        if Self::profile_visibility(&profile) == "hidden" {
            warnings.push(format!(
                "model profile '{}' is hidden and only intended for labs / compatibility testing.",
                self.config.model_profile
            ));
        }
        if Self::requires_acknowledgement(&profile) {
            warnings.push(format!(
                "model profile '{}' is not validated yet; deploys require --acknowledge-unvalidated until a validation artifact exists.",
                self.config.model_profile
            ));
        }
        if self.config.accelerator == Accelerator::Inf2 {
            warnings.push(
                "Inf2/Neuron is experimental in Emberlane; benchmark before claiming savings."
                    .to_string(),
            );
        }
        if self.config.ami_id.trim().is_empty() {
            warnings.push(
                "ami_id is empty; Emberlane will auto-select a DLAMI on deploy based on the accelerator, and you can override it with --ami-id."
                    .to_string(),
            );
        }
        let capacity_report = self.instance_type_offering_report().await?;
        if capacity_report.get("supported").and_then(Value::as_bool) == Some(false) {
            let mut message = format!(
                "instance type '{}' is not currently listed in region '{}'.",
                self.config.instance_type, self.config.region
            );
            let fallbacks = self.capacity_fallbacks();
            if !fallbacks.is_empty() {
                message.push_str(&format!(
                    " Suggested alternatives: {}.",
                    fallbacks.join(", ")
                ));
            }
            warnings.push(message);
        } else if capacity_report.get("supported").is_none() {
            warnings.push(
                "AWS instance-type availability check was skipped or unavailable; deploy may still fail if the region has temporary capacity shortage."
                    .to_string(),
            );
        }
        // Warn when deploying gated models without an HF token
        let gated_prefixes = [
            "meta-llama/",
            "google/gemma-",
            "mistralai/Mistral-",
            "mistralai/Mixtral-",
        ];
        if gated_prefixes
            .iter()
            .any(|p| profile.model_id.starts_with(p))
        {
            warnings.push(format!(
                "model '{}' is likely gated on HuggingFace and requires an HF token. Set hf_token_secret_arn or hf_token_ssm_parameter_name in your Terraform vars, otherwise vLLM will fail to download the model.",
                profile.model_id
            ));
        }
        Ok(json!({
            "provider": "aws",
            "config_path": self.path,
            "terraform_dir": self.config.terraform_dir,
            "model_profile": self.config.model_profile,
            "model_id": profile.model_id,
            "accelerator": self.config.accelerator,
            "instance_type": self.config.instance_type,
            "mode": self.config.mode,
            "capacity": capacity_report,
            "warnings": warnings,
            "future_backends": {"gcp": "not implemented", "azure": "not implemented"}
        }))
    }

    async fn render_deploy_vars(&self) -> Result<Value, EmberlaneError> {
        self.ensure_instance_type_available().await?;
        self.ensure_spot_quota_available().await?;
        let profile = profiles::profile(&self.config.model_profile)?;
        if Self::profile_visibility(&profile) == "hidden" && !self.config.allow_hidden_profiles {
            return Err(EmberlaneError::InvalidRequest(format!(
                "model profile '{}' is hidden; pass --experimental or --show-hidden to deploy it",
                self.config.model_profile
            )));
        }
        if Self::profile_visibility(&profile) == "hidden"
            && Self::requires_acknowledgement(&profile)
            && !self.config.acknowledge_unvalidated
        {
            return Err(EmberlaneError::InvalidRequest(format!(
                "model profile '{}' is not yet validated; pass --acknowledge-unvalidated to deploy it",
                self.config.model_profile
            )));
        }
        let mut vars = self.config.mode.terraform_values();
        let ami_id = self.resolve_ami_id().await?;
        let rope_scaling_json = profile
            .rope_scaling
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|err| {
                EmberlaneError::Internal(format!("failed to render rope scaling: {err}"))
            })?;
        let vllm_command = Self::build_vllm_command(&profile, self.config.accelerator)?;
        if vllm_command.trim().is_empty() {
            return Err(EmberlaneError::Internal(format!(
                "failed to render vLLM command for model profile '{}'",
                self.config.model_profile
            )));
        }
        let expected_model = shell_quote(&profile.model_id);
        if !vllm_command.starts_with(&format!("serve {expected_model} ")) {
            return Err(EmberlaneError::Internal(format!(
                "rendered vLLM command for model profile '{}' does not start with 'serve <model>': {}",
                self.config.model_profile, vllm_command
            )));
        }
        let obj = vars.as_object_mut().unwrap();
        obj.insert("app_name".to_string(), json!("emberlane"));
        obj.insert("aws_region".to_string(), json!(self.config.region));
        obj.insert("environment".to_string(), json!(self.config.environment));
        obj.insert("accelerator".to_string(), json!(self.config.accelerator));
        obj.insert(
            "runtime_pack".to_string(),
            json!(self.config.accelerator.runtime_pack()),
        );
        obj.insert(
            "model_profile".to_string(),
            json!(self.config.model_profile),
        );
        obj.insert("model_id".to_string(), json!(profile.model_id));
        obj.insert("max_model_len".to_string(), json!(profile.max_model_len));
        obj.insert(
            "quantization".to_string(),
            json!(profile.quantization.clone().unwrap_or_default()),
        );
        obj.insert(
            "rope_scaling_json".to_string(),
            json!(rope_scaling_json.unwrap_or_default()),
        );
        obj.insert(
            "gpu_memory_utilization".to_string(),
            json!(profile.gpu_memory_utilization.unwrap_or_default()),
        );
        obj.insert(
            "enforce_eager".to_string(),
            json!(profile.enforce_eager.unwrap_or(false)),
        );
        obj.insert(
            "max_num_seqs".to_string(),
            json!(profile.max_num_seqs.unwrap_or_default()),
        );
        obj.insert(
            "block_size".to_string(),
            json!(profile.block_size.unwrap_or_default()),
        );
        obj.insert(
            "num_gpu_blocks_override".to_string(),
            json!(profile
                .num_gpu_blocks_override
                .unwrap_or_else(|| profile.max_num_seqs.unwrap_or_default())),
        );
        obj.insert(
            "vllm_extra_args".to_string(),
            json!(profile.vllm_extra_args.join(" ")),
        );
        obj.insert("vllm_command".to_string(), json!(vllm_command));
        obj.insert(
            "language_model_only".to_string(),
            json!(profile.language_model_only),
        );
        obj.insert(
            "reasoning_parser".to_string(),
            json!(profile.reasoning_parser.clone().unwrap_or_default()),
        );
        obj.insert(
            "instance_type".to_string(),
            json!(self.config.instance_type),
        );
        obj.insert(
            "fallback_instance_types".to_string(),
            json!(self.spot_fallback_instance_types()),
        );
        obj.insert("ami_id".to_string(), json!(ami_id));
        obj.insert(
            "use_baked_ami".to_string(),
            json!(self.config.use_baked_ami),
        );
        obj.insert("public_alb".to_string(), json!(self.config.public_alb));
        obj.insert("api_key".to_string(), json!(self.config.api_key));
        obj.insert("artifact_prefix".to_string(), json!("emberlane/"));
        obj.insert(
            "emberlane_test_run".to_string(),
            json!(self.config.environment),
        );
        obj.insert(
            "visibility".to_string(),
            json!(Self::profile_visibility(&profile)),
        );
        obj.insert(
            "validation_status".to_string(),
            json!(profile
                .validation_status
                .clone()
                .unwrap_or_else(|| "needs_emberlane_validation".to_string())),
        );
        obj.insert("validated".to_string(), json!(profile.validated));
        obj.insert(
            "task_group".to_string(),
            json!(profile.task_group.clone().unwrap_or_default()),
        );
        obj.insert(
            "instance_group".to_string(),
            json!(profile.instance_group.clone().unwrap_or_default()),
        );
        obj.insert(
            "default_mode".to_string(),
            json!(profile.default_mode.clone().unwrap_or_default()),
        );
        obj.insert(
            "default_pricing".to_string(),
            json!(profile.default_pricing.clone().unwrap_or_default()),
        );
        obj.insert(
            "balanced_pricing".to_string(),
            json!(profile.balanced_pricing.clone().unwrap_or_default()),
        );
        obj.insert(
            "serving_modality".to_string(),
            json!(profile.serving_modality.clone().unwrap_or_default()),
        );
        if let Some(profile) = &self.config.profile {
            obj.insert("aws_profile".to_string(), json!(profile));
        }
        let enable_warm_pool = false;
        let enable_idle_scale_down = !matches!(self.config.mode, CostMode::AlwaysOn);
        let desired_capacity_on_sleep = if matches!(self.config.mode, CostMode::AlwaysOn) {
            1
        } else {
            0
        };
        let asg_desired_capacity = 1;
        obj.insert("enable_warm_pool".to_string(), json!(enable_warm_pool));
        obj.insert(
            "enable_idle_scale_down".to_string(),
            json!(enable_idle_scale_down),
        );
        obj.insert(
            "pytorch_cuda_alloc_conf".to_string(),
            json!("expandable_segments:True"),
        );
        obj.insert(
            "warm_pool_min_size".to_string(),
            json!(if enable_warm_pool { 1 } else { 0 }),
        );
        obj.insert(
            "asg_desired_capacity".to_string(),
            json!(asg_desired_capacity),
        );
        obj.insert("desired_capacity_on_wake".to_string(), json!(1));
        obj.insert(
            "desired_capacity_on_sleep".to_string(),
            json!(desired_capacity_on_sleep),
        );

        if let Some(token) = &self.config.hf_token {
            println!("[emberlane] Storing HuggingFace token securely in AWS SSM Parameter Store (/emberlane/hf-token) ...");
            let result = self
                .run_aws_cli(&[
                    "ssm",
                    "put-parameter",
                    "--name",
                    "/emberlane/hf-token",
                    "--value",
                    token,
                    "--type",
                    "SecureString",
                    "--overwrite",
                    "--region",
                    &self.config.region,
                ])
                .await?;
            if result["status"].as_i64().unwrap_or(1) != 0 {
                return Err(EmberlaneError::ProviderNotConfigured(format!(
                    "Failed to store HF token in SSM: {}",
                    result["stderr"].as_str().unwrap_or("unknown error")
                )));
            }
            obj.insert(
                "hf_token_ssm_parameter_name".to_string(),
                json!("/emberlane/hf-token"),
            );
        }
        Ok(vars)
    }

    async fn deploy(&self, auto_approve: bool, plan_only: bool) -> Result<Value, EmberlaneError> {
        let vars = self.render_deploy_vars().await?;
        fs::create_dir_all(&self.config.terraform_dir)?;
        fs::write(
            self.tfvars_path(),
            serde_json::to_string_pretty(&vars).unwrap(),
        )?;
        if plan_only {
            return Ok(json!({
                "ok": true,
                "plan_only": true,
                "tfvars": self.tfvars_path(),
                "vllm_command": vars.get("vllm_command").cloned().unwrap_or_else(|| json!("")),
                "message": "rendered Terraform variables; run terraform plan/apply when ready"
            }));
        }
        if !auto_approve {
            let summary = format!(
                "Deploy AWS {} runtime {} on {} in {} mode to region {} using {}",
                self.config.accelerator,
                self.config.model_profile,
                self.config.instance_type,
                self.config.mode,
                self.config.region,
                if self.config.ami_id.is_empty() {
                    "an auto-selected AMI"
                } else {
                    "the configured AMI"
                }
            );
            if !util::prompt_confirm(&summary)? {
                return Err(EmberlaneError::InvalidRequest(
                    "deployment cancelled by user".to_string(),
                ));
            }
        }
        let init = self.run_terraform(&["init"], true).await?;
        let apply = self
            .run_terraform(&["apply", "-auto-approve"], true)
            .await?;
        let terraform_outputs = self.run_terraform(&["output", "-json"], false).await.ok();
        let parsed_outputs = Self::terraform_outputs(
            terraform_outputs
                .as_ref()
                .and_then(|value| value.get("stdout").and_then(Value::as_str)),
        );
        let post_apply_wake = self
            .reconcile_asg_desired_capacity(parsed_outputs.as_ref(), &vars, &apply)
            .await?;
        let endpoint_url = self
            .run_terraform(&["output", "-raw", "lambda_function_url"], false)
            .await
            .ok()
            .and_then(|value| {
                value["stdout"]
                    .as_str()
                    .map(|s| s.trim().trim_matches('"').trim_end_matches('/').to_string())
            })
            .filter(|value| !value.is_empty());
        if let Some(ref endpoint_url) = endpoint_url {
            let mut rendered = self.to_file()?;
            rendered.deploy.endpoint_url = endpoint_url.clone();
            fs::write(
                &self.path,
                toml::to_string_pretty(&rendered).map_err(|err| {
                    EmberlaneError::Internal(format!("failed to persist AWS config: {err}"))
                })?,
            )?;
        }
        if let Some(outputs) = parsed_outputs.as_ref() {
            if let Some(bucket) = outputs
                .get("artifact_bucket_name")
                .and_then(|v| v.get("value"))
                .and_then(Value::as_str)
                .filter(|value| !value.trim().is_empty())
            {
                let emberlane_config_path = repo_root().join("emberlane.toml");
                if emberlane_config_path.exists() {
                    let mut cfg = EmberlaneConfig::discover(Some(emberlane_config_path.clone()))?;
                    cfg.storage.backend = StorageBackend::S3;
                    cfg.storage.s3 = Some(S3StorageConfig {
                        bucket: bucket.to_string(),
                        prefix: "uploads/".to_string(),
                        region: self.config.region.clone(),
                        aws_cli: "aws".to_string(),
                        profile: self.config.profile.clone(),
                        presign_downloads: true,
                        presign_expires_secs: 900,
                        pass_s3_uri: true,
                    });
                    cfg.write_to(emberlane_config_path, true)?;
                }
            }
        }
        Ok(json!({
            "tfvars": self.tfvars_path(),
            "init": init,
            "apply": apply,
            "post_apply_wake": post_apply_wake,
            "endpoint_url": endpoint_url
        }))
    }

    async fn destroy(&self, auto_approve: bool) -> Result<Value, EmberlaneError> {
        let prompt = "Destroy the current Emberlane AWS deployment?";
        let approved = auto_approve || util::prompt_confirm(prompt)?;
        if !approved {
            return Err(EmberlaneError::InvalidRequest(
                "destroy cancelled by user".to_string(),
            ));
        }
        self.run_terraform(&["destroy", "-auto-approve"], true)
            .await
    }

    async fn status(&self) -> Result<Value, EmberlaneError> {
        self.run_terraform(&["output", "-json"], false).await
    }

    async fn chat(&self, message: &str) -> Result<Value, EmberlaneError> {
        let endpoint = self.endpoint().await?;
        let alb_endpoint = self.alb_endpoint().await?;

        if let Some(alb_endpoint) = alb_endpoint.as_deref() {
            if alb_endpoint != endpoint {
                match self.post_chat(alb_endpoint, message, false).await {
                    Ok(alb_result) if !Self::should_wake_through_lambda(&alb_result) => {
                        let status = alb_result["status"].as_u64().unwrap_or(0);
                        return Ok(json!({
                            "status": status,
                            "body": alb_result["body"],
                            "endpoint": alb_result["endpoint"],
                            "wake_endpoint": endpoint
                        }));
                    }
                    Ok(_) | Err(_) => {
                        // The ALB has no ready target or is protected; use Lambda to wake/proxy.
                    }
                }
            }
        }

        let result = self.post_chat(&endpoint, message, true).await?;
        let status = result["status"].as_u64().unwrap_or(0);
        if matches!(status, 401 | 403) || Self::is_lambda_timeout(&result) {
            if let Some(alb_endpoint) = alb_endpoint {
                if alb_endpoint != endpoint {
                    let fallback = self.post_chat(&alb_endpoint, message, false).await?;
                    let fallback_status = fallback["status"].as_u64().unwrap_or(0);
                    if !matches!(fallback_status, 401 | 403) {
                        return Ok(json!({
                            "status": fallback_status,
                            "body": fallback["body"],
                            "endpoint": fallback["endpoint"],
                            "fallback": endpoint
                        }));
                    }
                }
            }
        }
        Ok(result)
    }

    async fn benchmark(&self) -> Result<Value, EmberlaneError> {
        let started = Instant::now();
        let result = self.chat(&self.benchmark_prompt).await;
        let elapsed_ms = started.elapsed().as_millis();
        let profile = profiles::profile(&self.config.model_profile)?;
        Ok(json!({
            "cloud_provider": "aws",
            "accelerator": self.config.accelerator,
            "model_profile": self.config.model_profile,
            "model_id": profile.model_id,
            "instance_type": self.config.instance_type,
            "mode": self.config.mode,
            "warm_pool_enabled": false,
            "elapsed_ms": elapsed_ms,
            "result": result.as_ref().ok().cloned(),
            "error": result.err().map(|e| e.to_string()),
            "caveats": [
                "Benchmark results are workload, AMI, model, region, quota, and warm-pool dependent.",
                "Emberlane does not claim fixed wake latency or savings from a single run."
            ]
        }))
    }

    async fn cost_report(&self) -> Result<Value, EmberlaneError> {
        let cache = pricing::load_cache(&self.config.region)?;
        let record = cache
            .as_ref()
            .and_then(|cache| cache.records.get(&self.config.instance_type));
        let economy = record.and_then(|record| pricing::estimate_hourly(record, true));
        let balanced = record.and_then(|record| pricing::estimate_hourly(record, false));
        Ok(json!({
            "cloud_provider": "aws",
            "accelerator": self.config.accelerator,
            "model_profile": self.config.model_profile,
            "instance_type": self.config.instance_type,
            "mode": self.config.mode,
            "pricing_configured": record.is_some(),
            "savings_claimed": false,
            "message": if record.is_some() {
                "Pricing cache loaded successfully; estimates are informational only."
            } else {
                "No pricing file is configured, so Emberlane will not claim savings."
            },
            "estimated_hourly": {
                "economy": economy.map(|p| json!({"hourly_usd": p.hourly_usd, "source": p.source})),
                "balanced": balanced.map(|p| json!({"hourly_usd": p.hourly_usd, "source": p.source}))
            },
            "comparison": [
                {"mode": "economy", "concept": "starts ready on Spot, then scales down after idle"},
                {"mode": "balanced", "concept": "starts ready on On-Demand, then scales down after idle"},
                {"mode": "always-on", "concept": "keeps one instance running and does not auto-scale down on idle"}
            ]
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::AwsBackend;

    #[test]
    fn spot_quota_family_maps_instance_types() {
        assert_eq!(
            AwsBackend::spot_quota_family_for_instance("g5.2xlarge"),
            Some("All G and VT Spot Instance Requests")
        );
        assert_eq!(
            AwsBackend::spot_quota_family_for_instance("g6e.xlarge"),
            Some("All G and VT Spot Instance Requests")
        );
        assert_eq!(
            AwsBackend::spot_quota_family_for_instance("inf2.xlarge"),
            Some("All Inf Spot Instance Requests")
        );
        assert_eq!(
            AwsBackend::spot_quota_family_for_instance("c6i.large"),
            None
        );
    }
}
