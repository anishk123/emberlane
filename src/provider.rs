use async_trait::async_trait;
use reqwest::Method;
use serde_json::{json, Value};
use std::{collections::HashMap, process::Stdio, sync::Arc};
use tokio::{
    process::{Child, Command},
    sync::Mutex,
    time::{sleep, Duration, Instant},
};

use crate::{
    error::EmberlaneError,
    model::{ChatRequest, ProviderKind, RouteRequest, RouteResponse, RuntimeConfig},
    util,
};

#[async_trait]
pub trait RuntimeProvider: Send + Sync {
    async fn wake(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError>;
    async fn sleep(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError>;
    async fn health(&self, runtime: &RuntimeConfig) -> Result<bool, EmberlaneError>;
    async fn route(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError>;
}

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, EmberlaneError>;
}

#[derive(Default)]
pub struct RealCommandRunner;

#[async_trait]
impl CommandRunner for RealCommandRunner {
    async fn run(&self, program: &str, args: &[String]) -> Result<CommandOutput, EmberlaneError> {
        let output = Command::new(program)
            .args(args)
            .output()
            .await
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    EmberlaneError::ProviderNotConfigured(format!(
                        "command '{program}' was not found; install the AWS CLI or set aws_cli"
                    ))
                } else {
                    EmberlaneError::Internal(format!("failed to run '{program}': {err}"))
                }
            })?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }
}

#[derive(Clone)]
pub struct ProviderRegistry {
    static_http: Arc<StaticHttpProvider>,
    local_process: Arc<LocalProcessProvider>,
    mock: Arc<MockProvider>,
    ollama: Arc<OllamaProvider>,
    aws_asg: Arc<AwsAsgProvider>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::with_command_runner(Arc::new(RealCommandRunner))
    }

    pub fn with_command_runner(runner: Arc<dyn CommandRunner>) -> Self {
        let static_http = Arc::new(StaticHttpProvider::default());
        Self {
            static_http: static_http.clone(),
            local_process: Arc::new(LocalProcessProvider::new(static_http.clone())),
            mock: Arc::new(MockProvider::default()),
            ollama: Arc::new(OllamaProvider::new(static_http.clone())),
            aws_asg: Arc::new(AwsAsgProvider::new(static_http, runner)),
        }
    }

    pub fn provider(&self, kind: &ProviderKind) -> Arc<dyn RuntimeProvider> {
        match kind {
            ProviderKind::StaticHttp => self.static_http.clone(),
            ProviderKind::LocalProcess => self.local_process.clone(),
            ProviderKind::Mock => self.mock.clone(),
            ProviderKind::Ollama => self.ollama.clone(),
            ProviderKind::AwsAsg => self.aws_asg.clone(),
        }
    }

    pub async fn aws_status(&self, runtime: &RuntimeConfig) -> Result<Value, EmberlaneError> {
        self.aws_asg.status(runtime).await
    }
}

#[derive(Default)]
pub struct StaticHttpProvider {
    client: reqwest::Client,
}

#[async_trait]
impl RuntimeProvider for StaticHttpProvider {
    async fn wake(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if self.health(runtime).await? {
            Ok(())
        } else {
            Err(EmberlaneError::WakeFailed(format!(
                "{} is not healthy; static_http cannot start runtimes",
                runtime.id
            )))
        }
    }

    async fn sleep(&self, _runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        Err(EmberlaneError::ProviderNotImplemented(
            "static_http sleep is not supported in v0.1".to_string(),
        ))
    }

    async fn health(&self, runtime: &RuntimeConfig) -> Result<bool, EmberlaneError> {
        let base = base_url(runtime)?;
        let url = util::join_url(base, &runtime.health_path);
        Ok(self
            .client
            .get(url)
            .timeout(Duration::from_secs(2))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false))
    }

    async fn route(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        proxy_json(&self.client, runtime, request).await
    }
}

pub struct LocalProcessProvider {
    children: Mutex<HashMap<String, Child>>,
    static_http: Arc<StaticHttpProvider>,
}

impl LocalProcessProvider {
    pub fn new(static_http: Arc<StaticHttpProvider>) -> Self {
        Self {
            children: Mutex::new(HashMap::new()),
            static_http,
        }
    }
}

#[async_trait]
impl RuntimeProvider for LocalProcessProvider {
    async fn wake(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if self.health(runtime).await? {
            return Ok(());
        }

        {
            let mut children = self.children.lock().await;
            if let Some(child) = children.get_mut(&runtime.id) {
                if child.try_wait()?.is_none() {
                    drop(children);
                    return poll_health(&*self.static_http, runtime, runtime.startup_timeout_secs)
                        .await;
                }
                children.remove(&runtime.id);
            }
        }

        let command = runtime
            .config
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("python");
        let child = match spawn_configured_command(runtime, command).await {
            Ok(child) => child,
            Err(err) if command == "python" => spawn_configured_command(runtime, "python3")
                .await
                .map_err(|_| err)?,
            Err(err) => return Err(err),
        };
        if let Some(pid) = child.id() {
            write_pid(&runtime.id, pid)?;
        }
        self.children.lock().await.insert(runtime.id.clone(), child);
        poll_health(&*self.static_http, runtime, runtime.startup_timeout_secs).await
    }

    async fn sleep(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if let Some(mut child) = self.children.lock().await.remove(&runtime.id) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = remove_pid(&runtime.id);
            return Ok(());
        }
        if let Some(pid) = read_pid(&runtime.id)? {
            let status = Command::new("kill").arg(pid.to_string()).status().await?;
            if status.success() {
                let _ = remove_pid(&runtime.id);
                return Ok(());
            }
        }
        Err(EmberlaneError::ProviderNotImplemented(
            "cannot sleep this local_process because Emberlane has no child handle or pid file"
                .to_string(),
        ))
    }

    async fn health(&self, runtime: &RuntimeConfig) -> Result<bool, EmberlaneError> {
        self.static_http.health(runtime).await
    }

    async fn route(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        self.static_http.route(runtime, request).await
    }
}

async fn spawn_configured_command(
    runtime: &RuntimeConfig,
    command: &str,
) -> Result<Child, EmberlaneError> {
    let mut cmd = Command::new(command);
    if let Some(args) = runtime.config.get("args").and_then(Value::as_array) {
        for arg in args.iter().filter_map(Value::as_str) {
            cmd.arg(arg);
        }
    }
    if let Some(cwd) = runtime.config.get("cwd").and_then(Value::as_str) {
        cmd.current_dir(cwd);
    }
    if let Some(env) = runtime.config.get("env").and_then(Value::as_object) {
        for (key, value) in env {
            if let Some(value) = value.as_str() {
                cmd.env(key, value);
            }
        }
    }
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    cmd.spawn().map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            EmberlaneError::WakeFailed(format!(
                "command '{command}' was not found while waking '{}'",
                runtime.id
            ))
        } else {
            EmberlaneError::WakeFailed(format!(
                "failed to start command '{command}' for '{}': {err}",
                runtime.id
            ))
        }
    })
}

#[derive(Default)]
pub struct MockProvider {
    ready: Mutex<HashMap<String, bool>>,
}

#[async_trait]
impl RuntimeProvider for MockProvider {
    async fn wake(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if runtime
            .config
            .get("should_fail")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(EmberlaneError::WakeFailed("mock wake failure".to_string()));
        }
        let delay = runtime
            .config
            .get("wake_delay_ms")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        if delay > 0 {
            sleep(Duration::from_millis(delay)).await;
        }
        self.ready.lock().await.insert(runtime.id.clone(), true);
        Ok(())
    }

    async fn sleep(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        self.ready.lock().await.insert(runtime.id.clone(), false);
        Ok(())
    }

    async fn health(&self, runtime: &RuntimeConfig) -> Result<bool, EmberlaneError> {
        Ok(*self.ready.lock().await.get(&runtime.id).unwrap_or(&false))
    }

    async fn route(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        if request.path == "/chat" {
            let chat: ChatRequest = serde_json::from_value(request.body)
                .map_err(|err| EmberlaneError::InvalidRequest(err.to_string()))?;
            let last = chat
                .messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.as_str())
                .unwrap_or("");
            return Ok(RouteResponse {
                status: 200,
                headers: HashMap::new(),
                body: json!({"reply": format!("Echo: {last}")}),
            });
        }
        if request.path == "/v1/chat/completions" {
            let messages = request
                .body
                .get("messages")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let last = messages
                .iter()
                .rev()
                .find(|m| m.get("role").and_then(Value::as_str) == Some("user"))
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            return Ok(RouteResponse {
                status: 200,
                headers: HashMap::new(),
                body: json!({
                    "id": "chatcmpl-echo",
                    "object": "chat.completion",
                    "model": request.body.get("model").and_then(Value::as_str).unwrap_or("echo"),
                    "choices": [{
                        "index": 0,
                        "message": {"role": "assistant", "content": format!("Echo: {last}")},
                        "finish_reason": "stop"
                    }]
                }),
            });
        }
        Ok(RouteResponse {
            status: 200,
            headers: HashMap::new(),
            body: json!({"runtime_id": runtime.id, "path": request.path, "body": request.body}),
        })
    }
}

pub struct OllamaProvider {
    client: reqwest::Client,
    static_http: Arc<StaticHttpProvider>,
    children: Mutex<HashMap<String, Child>>,
}

impl OllamaProvider {
    pub fn new(static_http: Arc<StaticHttpProvider>) -> Self {
        Self {
            client: reqwest::Client::new(),
            static_http,
            children: Mutex::new(HashMap::new()),
        }
    }

    async fn chat(
        &self,
        runtime: &RuntimeConfig,
        body: Value,
    ) -> Result<RouteResponse, EmberlaneError> {
        let mut request = body;
        let model = ollama_model(runtime);
        request["model"] = Value::String(model.clone());
        request["stream"] = Value::Bool(false);
        let url = util::join_url(base_url(runtime)?, "/api/chat");
        let resp = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await
            .map_err(|err| {
                EmberlaneError::RouteFailed(format!("failed to call Ollama /api/chat: {err}"))
            })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let body: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"text": text}));
        if status == 404
            || body
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|e| e.contains("model"))
        {
            return Err(EmberlaneError::RouteFailed(format!(
                "Ollama model '{model}' is not available; run `ollama pull {model}`"
            )));
        }
        let reply = body
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        Ok(RouteResponse {
            status,
            headers: HashMap::new(),
            body: json!({"reply": reply, "raw": body}),
        })
    }

    async fn openai_chat(
        &self,
        runtime: &RuntimeConfig,
        body: Value,
    ) -> Result<RouteResponse, EmberlaneError> {
        if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
            return Err(EmberlaneError::InvalidRequest(
                "streaming is not implemented in Emberlane v0.1".to_string(),
            ));
        }
        let model = body
            .get("model")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| ollama_model(runtime));
        let messages = body.get("messages").cloned().unwrap_or_else(|| json!([]));
        let url = util::join_url(base_url(runtime)?, "/api/chat");
        let resp = self
            .client
            .post(url)
            .json(&json!({"model": model, "messages": messages, "stream": false}))
            .send()
            .await
            .map_err(|err| EmberlaneError::RouteFailed(format!("failed to call Ollama: {err}")))?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        let raw: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"text": text}));
        if status == 404
            || raw
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|e| e.contains("model"))
        {
            return Err(EmberlaneError::RouteFailed(format!(
                "Ollama model '{model}' is not available; run `ollama pull {model}`"
            )));
        }
        let content = raw
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(Value::as_str)
            .unwrap_or("");
        Ok(RouteResponse {
            status,
            headers: HashMap::new(),
            body: json!({
                "id": "chatcmpl-ollama",
                "object": "chat.completion",
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": {"role": "assistant", "content": content},
                    "finish_reason": "stop"
                }]
            }),
        })
    }
}

#[async_trait]
impl RuntimeProvider for OllamaProvider {
    async fn wake(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if self.health(runtime).await? {
            return Ok(());
        }
        let command = runtime
            .config
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("ollama");
        let mut cmd = Command::new(command);
        if let Some(args) = runtime.config.get("args").and_then(Value::as_array) {
            for arg in args.iter().filter_map(Value::as_str) {
                cmd.arg(arg);
            }
        } else {
            cmd.arg("serve");
        }
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        let child = cmd.spawn().map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                EmberlaneError::WakeFailed(
                    "Ollama is not installed or not on PATH. Install Ollama, then run `ollama pull llama3.2:1b`.".to_string(),
                )
            } else {
                EmberlaneError::WakeFailed(format!("failed to start Ollama: {err}"))
            }
        })?;
        self.children.lock().await.insert(runtime.id.clone(), child);
        poll_health(&*self.static_http, runtime, runtime.startup_timeout_secs)
            .await
            .map_err(|_| {
                EmberlaneError::WakeFailed(
                    "Ollama did not become ready. Try running `ollama serve` in another terminal."
                        .to_string(),
                )
            })
    }

    async fn sleep(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if let Some(mut child) = self.children.lock().await.remove(&runtime.id) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Ok(())
        } else {
            Err(EmberlaneError::ProviderNotImplemented(
                "Emberlane can only sleep Ollama if this process started it".to_string(),
            ))
        }
    }

    async fn health(&self, runtime: &RuntimeConfig) -> Result<bool, EmberlaneError> {
        self.static_http.health(runtime).await
    }

    async fn route(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        match request.path.as_str() {
            "/chat" => self.chat(runtime, request.body).await,
            "/v1/chat/completions" => self.openai_chat(runtime, request.body).await,
            _ => proxy_json(&self.client, runtime, request).await,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwsAsgConfig {
    pub region: String,
    pub asg_name: String,
    pub desired_capacity_on_wake: i64,
    pub desired_capacity_on_sleep: i64,
    pub aws_cli: String,
    pub profile: Option<String>,
    pub warm_pool_expected: bool,
}

pub struct AwsAsgProvider {
    client: reqwest::Client,
    static_http: Arc<StaticHttpProvider>,
    runner: Arc<dyn CommandRunner>,
}

impl AwsAsgProvider {
    pub fn new(static_http: Arc<StaticHttpProvider>, runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            client: reqwest::Client::new(),
            static_http,
            runner,
        }
    }

    async fn set_desired_capacity(
        &self,
        cfg: &AwsAsgConfig,
        desired_capacity: i64,
    ) -> Result<(), EmberlaneError> {
        let mut args = vec![
            "autoscaling".to_string(),
            "set-desired-capacity".to_string(),
            "--auto-scaling-group-name".to_string(),
            cfg.asg_name.clone(),
            "--desired-capacity".to_string(),
            desired_capacity.to_string(),
            "--region".to_string(),
            cfg.region.clone(),
            "--no-honor-cooldown".to_string(),
        ];
        add_profile_args(&mut args, cfg);
        let output = self.runner.run(&cfg.aws_cli, &args).await?;
        if output.status == 0 {
            Ok(())
        } else {
            Err(EmberlaneError::WakeFailed(format!(
                "AWS CLI set-desired-capacity failed: {}",
                clean_stderr(&output.stderr)
            )))
        }
    }

    pub async fn status(&self, runtime: &RuntimeConfig) -> Result<Value, EmberlaneError> {
        let cfg = parse_aws_asg_config(runtime)?;
        let mut asg_args = vec![
            "autoscaling".to_string(),
            "describe-auto-scaling-groups".to_string(),
            "--auto-scaling-group-names".to_string(),
            cfg.asg_name.clone(),
            "--region".to_string(),
            cfg.region.clone(),
        ];
        add_profile_args(&mut asg_args, &cfg);
        let asg_output = self.runner.run(&cfg.aws_cli, &asg_args).await?;
        if asg_output.status != 0 {
            return Err(EmberlaneError::ProviderNotConfigured(format!(
                "failed to describe ASG '{}': {}",
                cfg.asg_name,
                clean_stderr(&asg_output.stderr)
            )));
        }
        let asg_json: Value = serde_json::from_str(&asg_output.stdout).map_err(|err| {
            EmberlaneError::InvalidRequest(format!("failed to parse AWS ASG status JSON: {err}"))
        })?;
        let mut status = parse_asg_status(&cfg, &asg_json)?;

        let mut warm_args = vec![
            "autoscaling".to_string(),
            "describe-warm-pool".to_string(),
            "--auto-scaling-group-name".to_string(),
            cfg.asg_name.clone(),
            "--region".to_string(),
            cfg.region.clone(),
        ];
        add_profile_args(&mut warm_args, &cfg);
        match self.runner.run(&cfg.aws_cli, &warm_args).await {
            Ok(output) if output.status == 0 => {
                let warm_json: Value = serde_json::from_str(&output.stdout).unwrap_or_else(
                    |_| json!({"raw": output.stdout, "warning": "failed to parse warm pool JSON"}),
                );
                status["warm_pool"] = parse_warm_pool_status(&warm_json);
            }
            Ok(output) => {
                status["warm_pool_warning"] = json!(format!(
                    "warm pool status unavailable: {}",
                    clean_stderr(&output.stderr)
                ));
            }
            Err(err) => {
                status["warm_pool_warning"] = json!(format!("warm pool status unavailable: {err}"));
            }
        }
        Ok(status)
    }
}

#[async_trait]
impl RuntimeProvider for AwsAsgProvider {
    async fn wake(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        if self.health(runtime).await? {
            return Ok(());
        }
        let cfg = parse_aws_asg_config(runtime)?;
        self.set_desired_capacity(&cfg, cfg.desired_capacity_on_wake)
            .await?;
        poll_health(self, runtime, runtime.startup_timeout_secs)
            .await
            .map_err(|_| {
                EmberlaneError::WakeFailed(format!(
                    "AWS ASG '{}' was scaled to desired capacity {}, but '{}' did not become healthy within {}s at {}{}",
                    cfg.asg_name,
                    cfg.desired_capacity_on_wake,
                    runtime.id,
                    runtime.startup_timeout_secs,
                    runtime.base_url.clone().unwrap_or_default(),
                    runtime.health_path
                ))
            })
    }

    async fn sleep(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        let cfg = parse_aws_asg_config(runtime)?;
        self.set_desired_capacity(&cfg, cfg.desired_capacity_on_sleep)
            .await
            .map_err(|err| EmberlaneError::SleepFailed(err.to_string()))
    }

    async fn health(&self, runtime: &RuntimeConfig) -> Result<bool, EmberlaneError> {
        self.static_http.health(runtime).await
    }

    async fn route(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        proxy_json(&self.client, runtime, request).await
    }
}

pub fn parse_aws_asg_config(runtime: &RuntimeConfig) -> Result<AwsAsgConfig, EmberlaneError> {
    if runtime.provider != ProviderKind::AwsAsg {
        return Err(EmberlaneError::InvalidRequest(format!(
            "runtime '{}' uses provider '{}', not aws_asg",
            runtime.id, runtime.provider
        )));
    }
    let region = runtime
        .config
        .get("region")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            EmberlaneError::ProviderNotConfigured(
                "aws_asg config requires region, for example region = \"us-west-2\"".to_string(),
            )
        })?
        .to_string();
    let asg_name = runtime
        .config
        .get("asg_name")
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| {
            EmberlaneError::ProviderNotConfigured("aws_asg config requires asg_name".to_string())
        })?
        .to_string();
    Ok(AwsAsgConfig {
        region,
        asg_name,
        desired_capacity_on_wake: runtime
            .config
            .get("desired_capacity_on_wake")
            .and_then(Value::as_i64)
            .unwrap_or(1),
        desired_capacity_on_sleep: runtime
            .config
            .get("desired_capacity_on_sleep")
            .and_then(Value::as_i64)
            .unwrap_or(0),
        aws_cli: runtime
            .config
            .get("aws_cli")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .unwrap_or("aws")
            .to_string(),
        profile: runtime
            .config
            .get("profile")
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
            .map(ToOwned::to_owned),
        warm_pool_expected: runtime
            .config
            .get("warm_pool_expected")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

pub fn aws_sample_config() -> &'static str {
    r#"[[runtimes]]
id = "aws-echo"
name = "AWS Echo Runtime"
provider = "aws_asg"
enabled = true
mode = "fast"
base_url = "http://your-alb-dns-name"
health_path = "/health"
startup_timeout_secs = 180
fast_wait_secs = 25
slow_retry_after_secs = 5
idle_ttl_secs = 300
max_concurrency = 2

[runtimes.config]
region = "us-west-2"
asg_name = "emberlane-echo-asg"
desired_capacity_on_wake = 1
desired_capacity_on_sleep = 0
aws_cli = "aws"
profile = ""
warm_pool_expected = true
"#
}

pub fn render_aws_iam_policy(runtime: &RuntimeConfig) -> Result<Value, EmberlaneError> {
    let cfg = parse_aws_asg_config(runtime)?;
    Ok(json!({
        "Version": "2012-10-17",
        "Statement": [{
            "Effect": "Allow",
            "Action": [
                "autoscaling:SetDesiredCapacity",
                "autoscaling:DescribeAutoScalingGroups",
                "autoscaling:DescribeWarmPool"
            ],
            "Resource": [
                format!("arn:aws:autoscaling:{}:<ACCOUNT_ID>:autoScalingGroup:*:autoScalingGroupName/{}", cfg.region, cfg.asg_name),
                "*"
            ],
            "Note": "Replace <ACCOUNT_ID>. DescribeAutoScalingGroups and DescribeWarmPool may require '*' depending on IAM evaluation."
        }]
    }))
}

fn parse_asg_status(cfg: &AwsAsgConfig, value: &Value) -> Result<Value, EmberlaneError> {
    let group = value
        .get("AutoScalingGroups")
        .and_then(Value::as_array)
        .and_then(|groups| groups.first())
        .ok_or_else(|| {
            EmberlaneError::ProviderNotConfigured(format!(
                "ASG '{}' was not found in region '{}'",
                cfg.asg_name, cfg.region
            ))
        })?;
    let instances = group
        .get("Instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let in_service_count = instances
        .iter()
        .filter(|i| i.get("LifecycleState").and_then(Value::as_str) == Some("InService"))
        .count();
    let pending_count = instances
        .iter()
        .filter(|i| {
            i.get("LifecycleState")
                .and_then(Value::as_str)
                .is_some_and(|s| s.contains("Pending"))
        })
        .count();
    Ok(json!({
        "provider": "aws_asg",
        "region": cfg.region,
        "asg_name": cfg.asg_name,
        "desired_capacity": group.get("DesiredCapacity").cloned().unwrap_or(Value::Null),
        "min_size": group.get("MinSize").cloned().unwrap_or(Value::Null),
        "max_size": group.get("MaxSize").cloned().unwrap_or(Value::Null),
        "in_service_count": in_service_count,
        "pending_count": pending_count,
        "instances": instances.iter().map(|i| json!({
            "instance_id": i.get("InstanceId").cloned().unwrap_or(Value::Null),
            "lifecycle_state": i.get("LifecycleState").cloned().unwrap_or(Value::Null),
            "health_status": i.get("HealthStatus").cloned().unwrap_or(Value::Null),
            "availability_zone": i.get("AvailabilityZone").cloned().unwrap_or(Value::Null)
        })).collect::<Vec<_>>()
    }))
}

fn parse_warm_pool_status(value: &Value) -> Value {
    let instances = value
        .get("Instances")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    json!({
        "configuration": value.get("WarmPoolConfiguration").cloned().unwrap_or(Value::Null),
        "instances": instances.iter().map(|i| json!({
            "instance_id": i.get("InstanceId").cloned().unwrap_or(Value::Null),
            "lifecycle_state": i.get("LifecycleState").cloned().unwrap_or(Value::Null),
            "health_status": i.get("HealthStatus").cloned().unwrap_or(Value::Null)
        })).collect::<Vec<_>>()
    })
}

fn add_profile_args(args: &mut Vec<String>, cfg: &AwsAsgConfig) {
    if let Some(profile) = &cfg.profile {
        args.push("--profile".to_string());
        args.push(profile.clone());
    }
}

fn clean_stderr(stderr: &str) -> String {
    let trimmed = stderr.trim();
    if trimmed.is_empty() {
        "no error output".to_string()
    } else {
        trimmed.to_string()
    }
}

async fn poll_health(
    provider: &dyn RuntimeProvider,
    runtime: &RuntimeConfig,
    timeout_secs: u64,
) -> Result<(), EmberlaneError> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if provider.health(runtime).await? {
            return Ok(());
        }
        sleep(Duration::from_millis(250)).await;
    }
    Err(EmberlaneError::WakeFailed(format!(
        "{} did not become healthy within {timeout_secs}s",
        runtime.id
    )))
}

async fn proxy_json(
    client: &reqwest::Client,
    runtime: &RuntimeConfig,
    request: RouteRequest,
) -> Result<RouteResponse, EmberlaneError> {
    let method = Method::from_bytes(request.method.as_bytes())
        .map_err(|_| EmberlaneError::InvalidRequest("invalid HTTP method".to_string()))?;
    let mut builder = client.request(method, util::join_url(base_url(runtime)?, &request.path));
    for (key, value) in request.headers {
        if !key.eq_ignore_ascii_case("host") && !key.eq_ignore_ascii_case("authorization") {
            builder = builder.header(key, value);
        }
    }
    if !request.body.is_null() {
        builder = builder.json(&request.body);
    }
    let resp = builder
        .send()
        .await
        .map_err(|err| EmberlaneError::RouteFailed(err.to_string()))?;
    let status = resp.status().as_u16();
    let headers = resp
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();
    let text = resp.text().await.unwrap_or_default();
    let body = serde_json::from_str(&text).unwrap_or_else(|_| json!({"text": text}));
    Ok(RouteResponse {
        status,
        headers,
        body,
    })
}

fn base_url(runtime: &RuntimeConfig) -> Result<&str, EmberlaneError> {
    runtime
        .base_url
        .as_deref()
        .ok_or_else(|| EmberlaneError::ProviderNotConfigured("base_url is required".to_string()))
}

fn ollama_model(runtime: &RuntimeConfig) -> String {
    runtime
        .config
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or("llama3.2:1b")
        .to_string()
}

fn pid_path(runtime_id: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(".emberlane")
        .join("pids")
        .join(format!("{runtime_id}.pid"))
}

fn write_pid(runtime_id: &str, pid: u32) -> Result<(), EmberlaneError> {
    let path = pid_path(runtime_id);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, pid.to_string())?;
    Ok(())
}

fn read_pid(runtime_id: &str) -> Result<Option<u32>, EmberlaneError> {
    let path = pid_path(runtime_id);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)?;
    text.trim()
        .parse::<u32>()
        .map(Some)
        .map_err(|err| EmberlaneError::SleepFailed(format!("invalid pid file: {err}")))
}

fn remove_pid(runtime_id: &str) -> Result<(), EmberlaneError> {
    let path = pid_path(runtime_id);
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_echo_runtime;
    use axum::{
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use std::{
        collections::VecDeque,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    #[derive(Default)]
    struct FakeCommandRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
        outputs: Mutex<VecDeque<CommandOutput>>,
    }

    impl FakeCommandRunner {
        async fn push_output(&self, output: CommandOutput) {
            self.outputs.lock().await.push_back(output);
        }

        async fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().await.clone()
        }
    }

    #[async_trait]
    impl CommandRunner for FakeCommandRunner {
        async fn run(
            &self,
            program: &str,
            args: &[String],
        ) -> Result<CommandOutput, EmberlaneError> {
            self.calls
                .lock()
                .await
                .push((program.to_string(), args.to_vec()));
            Ok(self
                .outputs
                .lock()
                .await
                .pop_front()
                .unwrap_or(CommandOutput {
                    status: 0,
                    stdout: "{}".to_string(),
                    stderr: String::new(),
                }))
        }
    }

    fn aws_runtime() -> RuntimeConfig {
        let mut rt = default_echo_runtime();
        rt.id = "aws-echo".to_string();
        rt.provider = ProviderKind::AwsAsg;
        rt.startup_timeout_secs = 2;
        rt.fast_wait_secs = 1;
        rt.config = json!({
            "region": "us-west-2",
            "asg_name": "emberlane-echo-asg",
            "desired_capacity_on_wake": 1,
            "desired_capacity_on_sleep": 0,
            "aws_cli": "aws",
            "profile": "dev",
            "warm_pool_expected": true
        });
        rt
    }

    #[tokio::test]
    async fn mock_provider_wake_success_and_failure() {
        let provider = MockProvider::default();
        let mut rt = default_echo_runtime();
        rt.provider = ProviderKind::Mock;
        rt.config = json!({});
        provider.wake(&rt).await.unwrap();
        assert!(provider.health(&rt).await.unwrap());
        provider.sleep(&rt).await.unwrap();
        assert!(!provider.health(&rt).await.unwrap());
        rt.config = json!({"should_fail": true});
        assert!(provider.wake(&rt).await.is_err());
    }

    #[tokio::test]
    async fn local_process_does_not_spawn_if_already_healthy() {
        let app = Router::new().route(
            "/health",
            axum::routing::get(|| async { Json(json!({"ok": true})) }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let static_http = Arc::new(StaticHttpProvider::default());
        let provider = LocalProcessProvider::new(static_http);
        let mut rt = default_echo_runtime();
        rt.base_url = Some(format!("http://{addr}"));
        rt.config = json!({"command": "definitely-not-a-real-command"});
        provider.wake(&rt).await.unwrap();
    }

    #[tokio::test]
    async fn ollama_translation_sets_stream_false() {
        let captured = Arc::new(Mutex::new(Value::Null));
        let captured_handler = captured.clone();
        let app = Router::new().route(
            "/api/chat",
            post(move |Json(body): Json<Value>| {
                let captured = captured_handler.clone();
                async move {
                    *captured.lock().await = body;
                    Json(json!({"message": {"role": "assistant", "content": "hi"}}))
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let provider = OllamaProvider::new(Arc::new(StaticHttpProvider::default()));
        let mut rt = default_echo_runtime();
        rt.provider = ProviderKind::Ollama;
        rt.base_url = Some(format!("http://{addr}"));
        rt.config = json!({"model": "llama3.2:1b"});
        let resp = provider
            .route(
                &rt,
                RouteRequest {
                    method: "POST".to_string(),
                    path: "/v1/chat/completions".to_string(),
                    headers: HashMap::new(),
                    body: json!({"model":"llama3.2:1b","messages":[{"role":"user","content":"hello"}],"stream":false}),
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.body["choices"][0]["message"]["content"], "hi");
        assert_eq!(captured.lock().await["stream"], false);
    }

    #[test]
    fn aws_asg_config_parsing_success_and_missing_asg() {
        let cfg = parse_aws_asg_config(&aws_runtime()).unwrap();
        assert_eq!(cfg.region, "us-west-2");
        assert_eq!(cfg.asg_name, "emberlane-echo-asg");
        assert_eq!(cfg.profile.as_deref(), Some("dev"));

        let mut rt = aws_runtime();
        rt.config = json!({"region": "us-west-2"});
        let err = parse_aws_asg_config(&rt).unwrap_err();
        assert!(matches!(err, EmberlaneError::ProviderNotConfigured(_)));
        assert!(err.to_string().contains("asg_name"));
    }

    #[tokio::test]
    async fn aws_wake_and_sleep_call_expected_commands() {
        let hits = Arc::new(AtomicUsize::new(0));
        let health_hits = hits.clone();
        let app = Router::new().route(
            "/health",
            get(move || {
                let health_hits = health_hits.clone();
                async move {
                    if health_hits.fetch_add(1, Ordering::SeqCst) == 0 {
                        StatusCode::SERVICE_UNAVAILABLE
                    } else {
                        StatusCode::OK
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let runner = Arc::new(FakeCommandRunner::default());
        let provider = AwsAsgProvider::new(Arc::new(StaticHttpProvider::default()), runner.clone());
        let mut rt = aws_runtime();
        rt.base_url = Some(format!("http://{addr}"));
        provider.wake(&rt).await.unwrap();
        provider.sleep(&rt).await.unwrap();
        let calls = runner.calls().await;
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].0, "aws");
        assert!(calls[0].1.contains(&"set-desired-capacity".to_string()));
        assert!(calls[0].1.contains(&"1".to_string()));
        assert!(calls[0].1.contains(&"--profile".to_string()));
        assert!(calls[1].1.contains(&"0".to_string()));
    }

    #[tokio::test]
    async fn aws_status_parses_asg_and_handles_warm_pool_failure() {
        let runner = Arc::new(FakeCommandRunner::default());
        runner
            .push_output(CommandOutput {
                status: 0,
                stdout: json!({
                    "AutoScalingGroups": [{
                        "AutoScalingGroupName": "emberlane-echo-asg",
                        "DesiredCapacity": 1,
                        "MinSize": 0,
                        "MaxSize": 1,
                        "Instances": [
                            {"InstanceId":"i-ready","LifecycleState":"InService","HealthStatus":"Healthy","AvailabilityZone":"us-west-2a"},
                            {"InstanceId":"i-pending","LifecycleState":"Pending","HealthStatus":"Healthy","AvailabilityZone":"us-west-2b"}
                        ]
                    }]
                })
                .to_string(),
                stderr: String::new(),
            })
            .await;
        runner
            .push_output(CommandOutput {
                status: 255,
                stdout: String::new(),
                stderr: "ValidationError: warm pool not found".to_string(),
            })
            .await;
        let provider = AwsAsgProvider::new(Arc::new(StaticHttpProvider::default()), runner);
        let status = provider.status(&aws_runtime()).await.unwrap();
        assert_eq!(status["desired_capacity"], 1);
        assert_eq!(status["in_service_count"], 1);
        assert_eq!(status["pending_count"], 1);
        assert!(status["warm_pool_warning"]
            .as_str()
            .unwrap()
            .contains("warm pool status unavailable"));
    }

    #[tokio::test]
    async fn aws_provider_health_and_route_use_base_url() {
        let app = Router::new()
            .route("/health", get(|| async { StatusCode::OK }))
            .route(
                "/chat",
                post(|Json(body): Json<Value>| async move { Json(json!({"received": body})) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let provider = AwsAsgProvider::new(
            Arc::new(StaticHttpProvider::default()),
            Arc::new(FakeCommandRunner::default()),
        );
        let mut rt = aws_runtime();
        rt.base_url = Some(format!("http://{addr}"));
        assert!(provider.health(&rt).await.unwrap());
        let resp = provider
            .route(
                &rt,
                RouteRequest {
                    method: "POST".to_string(),
                    path: "/chat".to_string(),
                    headers: HashMap::new(),
                    body: json!({"hello": "aws"}),
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.body["received"]["hello"], "aws");
    }

    #[test]
    fn aws_sample_config_and_iam_include_expected_actions() {
        assert!(aws_sample_config().contains("provider = \"aws_asg\""));
        let policy = render_aws_iam_policy(&aws_runtime()).unwrap();
        let actions = policy["Statement"][0]["Action"].as_array().unwrap();
        assert!(actions
            .iter()
            .any(|a| a == "autoscaling:SetDesiredCapacity"));
        assert!(actions.iter().any(|a| a == "autoscaling:DescribeWarmPool"));
    }
}
