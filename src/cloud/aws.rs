use super::{
    model::{repo_root, Accelerator, CloudBackend, CloudDeployConfig, CloudProvider},
    modes::CostMode,
    profiles,
};
use crate::{error::EmberlaneError, util};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Stdio, time::Instant};
use tokio::process::Command;

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

    fn to_file(&self) -> AwsFile {
        AwsFile {
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
        }
    }

    #[allow(dead_code)]
    pub fn default_config_text() -> Result<String, EmberlaneError> {
        let backend = Self::from_config(aws_config_path(), CloudDeployConfig::default());
        toml::to_string_pretty(&backend.to_file())
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
        let url = if let Ok(url) = std::env::var("EMBERLANE_AWS_ENDPOINT") {
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
        let text = toml::to_string_pretty(&self.to_file()).map_err(|err| {
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
            "warnings": warnings,
            "future_backends": {"gcp": "not implemented", "azure": "not implemented"}
        }))
    }

    async fn render_deploy_vars(&self) -> Result<Value, EmberlaneError> {
        let profile = profiles::profile(&self.config.model_profile)?;
        let mut vars = self.config.mode.terraform_values();
        let ami_id = self.resolve_ami_id().await?;
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
        obj.insert(
            "instance_type".to_string(),
            json!(self.config.instance_type),
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
        if let Some(profile) = &self.config.profile {
            obj.insert("aws_profile".to_string(), json!(profile));
        }
        let enable_warm_pool = matches!(self.config.mode, CostMode::Balanced);
        obj.insert("enable_warm_pool".to_string(), json!(enable_warm_pool));
        obj.insert(
            "warm_pool_min_size".to_string(),
            json!(if enable_warm_pool { 1 } else { 0 }),
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
        Ok(json!({"tfvars": self.tfvars_path(), "init": init, "apply": apply}))
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
        let profile = profiles::profile(&self.config.model_profile)?;
        let body = json!({
            "model": profile.model_id,
            "messages": [{"role": "user", "content": message}],
            "stream": false
        });
        let mut req = reqwest::Client::new()
            .post(format!("{endpoint}/v1/chat/completions"))
            .json(&body);
        if let Some(api_key) = &self.config.api_key {
            req = req.bearer_auth(api_key).header("x-api-key", api_key);
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let json_body = resp.json::<Value>().await.unwrap_or_else(|_| json!({}));
        Ok(json!({"status": status, "body": json_body}))
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
            "warm_pool_enabled": self.config.mode == CostMode::Balanced,
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
        Ok(json!({
            "cloud_provider": "aws",
            "accelerator": self.config.accelerator,
            "model_profile": self.config.model_profile,
            "instance_type": self.config.instance_type,
            "mode": self.config.mode,
            "pricing_configured": false,
            "savings_claimed": false,
            "message": "No pricing file is configured, so Emberlane will not claim savings.",
            "comparison": [
                {"mode": "economy", "concept": "lowest idle infrastructure cost; coldest wake path"},
                {"mode": "balanced", "concept": "uses Warm Pool; may trade storage/prepared-capacity cost for warmer starts"},
                {"mode": "always-on", "concept": "keeps one instance running; highest idle cost and fastest response"}
            ]
        }))
    }
}
