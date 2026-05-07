use crate::{
    cloud::{model::CloudBackend, profiles, AwsBackend},
    config::EmberlaneConfig,
    error::EmberlaneError,
    mcp,
    model::{ChatMessage, ChatRequest},
    router::RuntimeRouter,
    storage::Storage,
    util,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Instant,
};
use tokio::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

const AWS_OPT_IN_MESSAGE: &str = "This test creates billable AWS resources. Rerun with --yes-i-understand-this-creates-aws-resources or set EMBERLANE_ALLOW_AWS_TESTS=1.";

pub struct HarnessReport {
    pub summary: Value,
    pub markdown_path: PathBuf,
}

pub struct LocalTestOptions {
    pub runtime: String,
    pub file: Option<PathBuf>,
    pub skip_mcp: bool,
    pub skip_http: bool,
    pub allow_missing_ollama: bool,
}

pub struct AwsTestOptions {
    pub models: String,
    pub accelerator: String,
    pub instance: Option<String>,
    pub mode: String,
    pub region: Option<String>,
    pub profile: Option<String>,
    pub ami_id: Option<String>,
    pub destroy: bool,
    pub keep_on_failure: bool,
    pub auto_approve: bool,
    pub max_wait_secs: u64,
    pub skip_streaming: bool,
    pub skip_file: bool,
    pub skip_cost: bool,
    pub allow: bool,
}

pub struct AwsMatrixOptions {
    pub config: PathBuf,
    pub only: Option<String>,
    pub exclude_experimental: bool,
    pub destroy: bool,
    pub auto_approve: bool,
    pub allow: bool,
}

pub struct CleanupOptions {
    pub environment: Option<String>,
    pub test_run: Option<PathBuf>,
    pub force: bool,
    pub dry_run: bool,
    pub delete_bucket_contents: bool,
}

#[derive(Debug, Deserialize)]
struct MatrixFile {
    cases: Vec<MatrixCase>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MatrixCase {
    pub name: String,
    pub model: String,
    pub accelerator: String,
    pub instance: String,
    pub mode: String,
    #[serde(default)]
    pub experimental: bool,
}

pub async fn run_local_test(
    config: Option<PathBuf>,
    opts: LocalTestOptions,
) -> Result<HarnessReport, EmberlaneError> {
    let started = util::now();
    let cfg = EmberlaneConfig::discover(config)?;
    fs::create_dir_all(cfg.files_dir())?;
    let storage = Storage::open(cfg.db_path())?;
    let router = RuntimeRouter::new(cfg.clone(), storage);
    router.seed_config_runtimes()?;

    let run_dir = cfg
        .server
        .data_dir
        .join("test-runs")
        .join("local")
        .join(timestamp());
    fs::create_dir_all(&run_dir)?;

    let mut steps = Vec::new();
    let mut ok = true;

    steps.push(step(
        "status",
        true,
        json!(router.status(&opts.runtime).await?),
    ));

    match router
        .chat(&opts.runtime, chat_request("hello from emberlane test"))
        .await
    {
        Ok(resp) => steps.push(step("chat", true, resp.body)),
        Err(err) if opts.runtime == "ollama" && opts.allow_missing_ollama => {
            steps.push(step("chat", true, json!({"skipped": err.to_string()})));
        }
        Err(err) => {
            ok = false;
            steps.push(step("chat", false, json!({"error": ollama_hint(&err)})));
        }
    }

    if opts.skip_http {
        steps.push(step("http_healthz", true, json!({"skipped": true})));
    } else {
        let url = format!("http://{}:{}/healthz", cfg.server.host, cfg.server.port);
        match reqwest::get(&url).await {
            Ok(resp) => steps.push(step(
                "http_healthz",
                resp.status().is_success(),
                json!({"url": url, "status": resp.status().as_u16()}),
            )),
            Err(err) => steps.push(step(
                "http_healthz",
                true,
                json!({"skipped": "server was not running", "url": url, "error": err.to_string()}),
            )),
        }
    }

    let test_file = if let Some(path) = opts.file {
        path
    } else {
        let path = run_dir.join("local-test.md");
        fs::write(
            &path,
            "# Emberlane local test\n\nThis file verifies chat-file.\n",
        )?;
        path
    };
    match router.upload_path(&test_file).await {
        Ok(file) => {
            steps.push(step("upload", true, json!(file)));
            match router
                .chat_file(&opts.runtime, &file.id, "summarize this file")
                .await
            {
                Ok(resp) => steps.push(step("chat_file", true, resp.body)),
                Err(err) if opts.runtime == "ollama" && opts.allow_missing_ollama => {
                    steps.push(step("chat_file", true, json!({"skipped": err.to_string()})));
                }
                Err(err) => {
                    ok = false;
                    steps.push(step(
                        "chat_file",
                        false,
                        json!({"error": ollama_hint(&err)}),
                    ));
                }
            }
        }
        Err(err) => {
            ok = false;
            steps.push(step("upload", false, json!({"error": err.to_string()})));
        }
    }

    if opts.skip_mcp {
        steps.push(step("mcp", true, json!({"skipped": true})));
    } else {
        let tools = mcp::dispatch_value(
            router.clone(),
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
        )
        .await;
        let chat = mcp::dispatch_value(
            router,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"emberlane_chat","arguments":{"runtime_id":opts.runtime,"message":"hello"}}}),
        )
        .await;
        let mcp_ok = chat["result"]["isError"].as_bool() == Some(false)
            || (opts.runtime == "ollama" && opts.allow_missing_ollama);
        ok &= mcp_ok;
        steps.push(step("mcp", mcp_ok, json!({"tools": tools, "chat": chat})));
    }

    let summary = json!({
        "kind": "local",
        "ok": ok,
        "runtime": opts.runtime,
        "started_at": started,
        "finished_at": util::now(),
        "steps": steps
    });
    write_report(&run_dir, "report", &summary)?;
    if !ok {
        return Err(EmberlaneError::Internal(format!(
            "local test failed; report: {}",
            run_dir.join("report.md").display()
        )));
    }
    Ok(HarnessReport {
        summary,
        markdown_path: run_dir.join("report.md"),
    })
}

pub async fn run_aws_test(opts: AwsTestOptions) -> Result<HarnessReport, EmberlaneError> {
    ensure_aws_opt_in(opts.allow)?;
    let started = util::now();
    let run_id = timestamp();
    let run_dir = PathBuf::from(".emberlane")
        .join("test-runs")
        .join("aws")
        .join(&run_id);
    fs::create_dir_all(&run_dir)?;

    let credentials = check_aws_credentials(opts.profile.clone(), opts.region.clone()).await?;
    if credentials["ok"].as_bool() != Some(true) {
        let summary = json!({
            "kind": "aws",
            "ok": false,
            "started_at": started,
            "finished_at": util::now(),
            "credentials": credentials
        });
        write_report(&run_dir, "summary", &summary)?;
        let guidance = credentials["message"]
            .as_str()
            .unwrap_or("No AWS credentials were found. Run: emberlane aws credentials check");
        return Err(EmberlaneError::ProviderNotConfigured(format!(
            "AWS credentials are not configured.\n\n{guidance}\n\nsummary: {}",
            run_dir.join("summary.md").display()
        )));
    }
    let mut cases = Vec::new();
    let mut all_ok = true;
    for model in split_models(&opts.models) {
        let case_report = run_aws_case(&run_dir, &run_id, &model, &opts).await;
        match case_report {
            Ok(report) => {
                all_ok &= report["ok"].as_bool().unwrap_or(false);
                cases.push(report);
            }
            Err(err) => {
                all_ok = false;
                cases.push(json!({"model": model, "ok": false, "error": err.to_string()}));
            }
        }
    }
    let summary = json!({
        "kind": "aws",
        "ok": all_ok,
        "started_at": started,
        "finished_at": util::now(),
        "credentials": credentials,
        "cases": cases
    });
    write_report(&run_dir, "summary", &summary)?;
    if !all_ok {
        return Err(EmberlaneError::Internal(format!(
            "AWS test failed; summary: {}",
            run_dir.join("summary.md").display()
        )));
    }
    Ok(HarnessReport {
        summary,
        markdown_path: run_dir.join("summary.md"),
    })
}

async fn run_aws_case(
    run_dir: &Path,
    run_id: &str,
    model: &str,
    opts: &AwsTestOptions,
) -> Result<Value, EmberlaneError> {
    let safe_model = sanitize(model);
    let env_name = format!("emberlane-it-{safe_model}-{run_id}");
    let case_dir = run_dir.join(&safe_model);
    fs::create_dir_all(&case_dir)?;

    let mut backend = AwsBackend::load_or_default(None)?.with_overrides(
        Some(model.to_string()),
        Some(opts.accelerator.clone()),
        opts.instance.clone(),
        Some(opts.mode.clone()),
        None,
    )?;
    backend.config.environment = env_name.clone();
    if let Some(region) = &opts.region {
        backend.config.region = region.clone();
    }
    if let Some(profile) = &opts.profile {
        backend.config.profile = Some(profile.clone());
    }
    if let Some(ami_id) = &opts.ami_id {
        backend.config.ami_id = ami_id.clone();
    }

    let mut steps = Vec::new();
    let mut ok = true;
    if !opts.auto_approve {
        steps.push(step(
            "safety",
            true,
            json!({"note": "AWS test harness uses terraform -auto-approve after explicit billable-resource opt-in."}),
        ));
    }
    let tfvars = backend.render_deploy_vars().await?;
    steps.push(step("render_tfvars", true, tfvars.clone()));

    let deploy = backend.deploy(true, false).await;
    match deploy {
        Ok(value) => {
            write_text(&case_dir.join("terraform.log"), &value.to_string())?;
            let deploy_ok = terraform_result_ok(&value);
            ok &= deploy_ok;
            steps.push(step("deploy", deploy_ok, value));
        }
        Err(err) => {
            ok = false;
            steps.push(step("deploy", false, json!({"error": err.to_string()})));
        }
    }

    if ok {
        let ready_started = Instant::now();
        let chat = retry_until(opts.max_wait_secs, || async {
            backend
                .chat("hello from Emberlane AWS integration test")
                .await
        })
        .await;
        steps.push(step(
            "chat",
            chat.is_ok(),
            chat.unwrap_or_else(|err| json!({"error": err.to_string()})),
        ));
        ok &= steps.last().unwrap()["ok"].as_bool().unwrap_or(false);
        steps.push(step(
            "wait_ready",
            ok,
            json!({"elapsed_ms": ready_started.elapsed().as_millis()}),
        ));
    }

    if ok {
        let openai = backend.chat("OpenAI-compatible test").await;
        steps.push(step(
            "openai_chat",
            openai.is_ok(),
            openai.unwrap_or_else(|err| json!({"error": err.to_string()})),
        ));
        ok &= steps.last().unwrap()["ok"].as_bool().unwrap_or(false);
    }

    if opts.skip_streaming {
        steps.push(step("streaming", true, json!({"skipped": true})));
    } else if ok {
        let streaming = streaming_probe(&backend).await;
        steps.push(step(
            "streaming",
            streaming.is_ok(),
            streaming.unwrap_or_else(|err| json!({"error": err.to_string()})),
        ));
    }

    if opts.skip_file {
        steps.push(step("file", true, json!({"skipped": true})));
    } else {
        steps.push(step(
            "file",
            true,
            json!({"skipped": "AWS file-chat requires an application runtime contract; S3 artifact routing is tested separately."}),
        ));
    }

    if ok {
        let benchmark = backend.benchmark().await;
        steps.push(step(
            "benchmark",
            benchmark.is_ok(),
            benchmark.unwrap_or_else(|err| json!({"error": err.to_string()})),
        ));
    }
    if !opts.skip_cost {
        steps.push(step("cost_report", true, backend.cost_report().await?));
    }

    let diagnostics = diagnose_aws(Some(backend.config.terraform_dir.clone())).await?;
    fs::write(
        case_dir.join("diagnostics.json"),
        serde_json::to_string_pretty(&util::redact_value(&diagnostics)).unwrap(),
    )?;

    if opts.destroy && (ok || !opts.keep_on_failure) {
        let destroy = backend.destroy(true).await;
        steps.push(step(
            "destroy",
            destroy.is_ok(),
            destroy.unwrap_or_else(|err| json!({"error": err.to_string()})),
        ));
    }

    let report = json!({
        "kind": "aws_case",
        "ok": ok,
        "model": model,
        "environment": env_name,
        "auto_approve": opts.auto_approve,
        "destroy_requested": opts.destroy,
        "steps": steps
    });
    write_report(&case_dir, "report", &report)?;
    Ok(report)
}

pub async fn run_aws_matrix_test(opts: AwsMatrixOptions) -> Result<HarnessReport, EmberlaneError> {
    ensure_aws_opt_in(opts.allow)?;
    let cases = load_matrix(&opts.config)?;
    let selected = select_matrix_cases(cases, opts.only.as_deref(), opts.exclude_experimental);
    if selected.is_empty() {
        return Err(EmberlaneError::InvalidRequest(
            "matrix selection produced no cases".to_string(),
        ));
    }
    let mut reports = Vec::new();
    let run_id = timestamp();
    let run_dir = PathBuf::from(".emberlane")
        .join("test-runs")
        .join("aws")
        .join(&run_id);
    fs::create_dir_all(&run_dir)?;
    for case in selected {
        let report = run_aws_test(AwsTestOptions {
            models: case.model,
            accelerator: case.accelerator,
            instance: Some(case.instance),
            mode: case.mode,
            region: None,
            profile: None,
            ami_id: None,
            destroy: opts.destroy,
            keep_on_failure: false,
            auto_approve: opts.auto_approve,
            max_wait_secs: 1800,
            skip_streaming: false,
            skip_file: false,
            skip_cost: false,
            allow: true,
        })
        .await;
        reports.push(match report {
            Ok(report) => report.summary,
            Err(err) => json!({"ok": false, "error": err.to_string()}),
        });
    }
    let ok = reports.iter().all(|r| r["ok"].as_bool() == Some(true));
    let summary = json!({"kind":"aws_matrix","ok":ok,"cases":reports});
    write_report(&run_dir, "summary", &summary)?;
    if !ok {
        return Err(EmberlaneError::Internal(format!(
            "AWS matrix failed; summary: {}",
            run_dir.join("summary.md").display()
        )));
    }
    Ok(HarnessReport {
        summary,
        markdown_path: run_dir.join("summary.md"),
    })
}

pub async fn check_aws_credentials(
    profile: Option<String>,
    region: Option<String>,
) -> Result<Value, EmberlaneError> {
    let aws_cli = command_exists("aws").await;
    if !aws_cli {
        return Ok(credentials_failure("AWS CLI was not found"));
    }
    let region = region
        .or_else(|| std::env::var("AWS_REGION").ok())
        .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
        .unwrap_or_else(|| "us-west-2".to_string());
    let profile = profile.or_else(|| std::env::var("AWS_PROFILE").ok());
    let mut args = vec![
        "sts".to_string(),
        "get-caller-identity".to_string(),
        "--region".to_string(),
        region.clone(),
    ];
    if let Some(profile) = &profile {
        args.push("--profile".to_string());
        args.push(profile.clone());
    }
    let output = Command::new("aws")
        .args(&args)
        .output()
        .await
        .map_err(|err| EmberlaneError::Internal(err.to_string()))?;
    if !output.status.success() {
        return Ok(credentials_failure(&String::from_utf8_lossy(
            &output.stderr,
        )));
    }
    Ok(credentials_result_from_stdout(
        &String::from_utf8_lossy(&output.stdout),
        profile,
        region,
    ))
}

pub fn credentials_result_from_stdout(
    stdout: &str,
    profile: Option<String>,
    region: String,
) -> Value {
    let value: Value = serde_json::from_str(stdout).unwrap_or_else(|_| json!({}));
    json!({
        "ok": true,
        "account_id": value.get("Account").cloned().unwrap_or(Value::Null),
        "arn": value.get("Arn").cloned().unwrap_or(Value::Null),
        "user_id": value.get("UserId").cloned().unwrap_or(Value::Null),
        "region": region,
        "profile": profile.unwrap_or_else(|| "default/environment".to_string())
    })
}

fn credentials_failure(reason: &str) -> Value {
    json!({
        "ok": false,
        "reason": reason.trim(),
        "message": "No AWS credentials were found.\n\nConfigure one of the following:\n\n1. AWS CLI login profile:\n   aws login --profile emberlane\n   emberlane aws init --profile emberlane\n\n2. Default profile:\n   aws configure\n\n3. Named profile:\n   aws configure --profile emberlane-dev\n   emberlane aws init --profile emberlane-dev\n\n4. SSO:\n   aws configure sso\n   aws sso login --profile <profile>\n\n5. Environment variables:\n   export AWS_ACCESS_KEY_ID=...\n   export AWS_SECRET_ACCESS_KEY=...\n   export AWS_REGION=us-west-2\n\nThen run:\n   emberlane aws credentials check --profile emberlane"
    })
}

pub async fn diagnose_aws(deployment: Option<PathBuf>) -> Result<Value, EmberlaneError> {
    let dir = deployment.unwrap_or_else(|| repo_root().join("infra/terraform"));
    let sts = check_aws_credentials(None, None)
        .await
        .unwrap_or_else(|err| json!({"ok": false, "error": err.to_string()}));
    let terraform_output = run_optional_command(
        "terraform",
        &["output".to_string(), "-json".to_string()],
        Some(&dir),
    )
    .await;
    Ok(util::redact_value(&json!({
        "sts": sts,
        "terraform_dir": dir,
        "terraform_output": terraform_output,
        "notes": [
            "ASG, target group, ALB, Lambda, and instance diagnostics are available after Terraform output is present.",
            "Secrets, API keys, authorization headers, and presigned URLs are redacted from reports."
        ]
    })))
}

pub async fn cleanup_aws(opts: CleanupOptions) -> Result<Value, EmberlaneError> {
    let env = opts.environment.unwrap_or_else(|| "dev".to_string());
    let mut actions = vec![json!({
        "action": "scan_tagged_resources",
        "tags": {"App": "emberlane", "Environment": env, "ManagedBy": "emberlane", "EmberlaneTestRun": env},
        "dry_run": opts.dry_run
    })];
    if let Some(test_run) = &opts.test_run {
        actions.push(
            json!({"action": "terraform_destroy", "path": test_run, "dry_run": opts.dry_run}),
        );
        if opts.force && !opts.dry_run {
            let result = run_optional_command(
                "terraform",
                &["destroy".to_string(), "-auto-approve".to_string()],
                Some(test_run),
            )
            .await;
            actions.push(json!({"action": "terraform_destroy_result", "result": result}));
        }
    }
    if opts.delete_bucket_contents && !opts.force {
        actions.push(json!({"warning": "--delete-bucket-contents requires --force"}));
    }
    Ok(json!({
        "ok": true,
        "dry_run": opts.dry_run,
        "force": opts.force,
        "delete_bucket_contents": opts.delete_bucket_contents,
        "actions": actions,
        "policy": "Conservative cleanup only deletes known Terraform deployments or tagged Emberlane resources."
    }))
}

fn ensure_aws_opt_in(allow: bool) -> Result<(), EmberlaneError> {
    if allow || std::env::var("EMBERLANE_ALLOW_AWS_TESTS").ok().as_deref() == Some("1") {
        Ok(())
    } else {
        Err(EmberlaneError::InvalidRequest(
            AWS_OPT_IN_MESSAGE.to_string(),
        ))
    }
}

fn load_matrix(path: &Path) -> Result<Vec<MatrixCase>, EmberlaneError> {
    let text = fs::read_to_string(path)?;
    let parsed: MatrixFile = toml::from_str(&text).map_err(|err| {
        EmberlaneError::InvalidRequest(format!("failed to parse AWS matrix config: {err}"))
    })?;
    Ok(parsed.cases)
}

fn select_matrix_cases(
    cases: Vec<MatrixCase>,
    only: Option<&str>,
    exclude_experimental: bool,
) -> Vec<MatrixCase> {
    cases
        .into_iter()
        .filter(|case| only.map(|only| case.name == only).unwrap_or(true))
        .filter(|case| !(exclude_experimental && case.experimental))
        .collect()
}

fn split_models(models: &str) -> Vec<String> {
    models
        .split(',')
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn step(name: &str, ok: bool, details: Value) -> Value {
    json!({"name": name, "ok": ok, "details": util::redact_value(&details)})
}

fn chat_request(message: &str) -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: message.to_string(),
        }],
        files: vec![],
    }
}

fn ollama_hint(err: &EmberlaneError) -> String {
    let message = err.to_string();
    if message.to_ascii_lowercase().contains("ollama") {
        format!("{message}. If Ollama is unavailable, install Ollama, run `ollama serve`, and run `ollama pull llama3.2:1b`.")
    } else {
        message
    }
}

fn write_report(dir: &Path, name: &str, summary: &Value) -> Result<(), EmberlaneError> {
    fs::create_dir_all(dir)?;
    let redacted = util::redact_value(summary);
    fs::write(
        dir.join(format!("{name}.json")),
        serde_json::to_string_pretty(&redacted).unwrap(),
    )?;
    fs::write(dir.join(format!("{name}.md")), markdown_report(&redacted))?;
    Ok(())
}

fn markdown_report(value: &Value) -> String {
    format!(
        "# Emberlane Test Report\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(value).unwrap()
    )
}

fn write_text(path: &Path, text: &str) -> Result<(), EmberlaneError> {
    fs::write(path, util::redact_text(text))?;
    Ok(())
}

fn terraform_result_ok(value: &Value) -> bool {
    value
        .get("init")
        .and_then(|v| v.get("status"))
        .and_then(Value::as_i64)
        .unwrap_or(1)
        == 0
        && value
            .get("apply")
            .and_then(|v| v.get("status"))
            .and_then(Value::as_i64)
            .unwrap_or(1)
            == 0
}

fn timestamp() -> String {
    util::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

async fn command_exists(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn run_optional_command(command: &str, args: &[String], cwd: Option<&Path>) -> Value {
    let mut cmd = Command::new(command);
    cmd.args(args);
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    match cmd.output().await {
        Ok(output) => util::redact_value(&json!({
            "status": output.status.code().unwrap_or(1),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr)
        })),
        Err(err) => json!({"error": err.to_string()}),
    }
}

async fn retry_until<F, Fut>(max_wait_secs: u64, mut f: F) -> Result<Value, EmberlaneError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<Value, EmberlaneError>>,
{
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(max_wait_secs);
    let mut last_err = None;
    while std::time::Instant::now() < deadline {
        match f().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| EmberlaneError::Internal("timed out".to_string())))
}

async fn streaming_probe(backend: &AwsBackend) -> Result<Value, EmberlaneError> {
    let profile = profiles::profile(&backend.config.model_profile)?;
    let endpoint = backend.endpoint_url().await?;
    let mut req = reqwest::Client::new()
        .post(format!(
            "{}/v1/chat/completions",
            endpoint.trim_end_matches('/')
        ))
        .json(&json!({
            "model": profile.model_id,
            "messages": [{"role":"user","content":"stream one sentence"}],
            "stream": true
        }));
    if let Some(api_key) = &backend.config.api_key {
        req = req.bearer_auth(api_key);
    }
    let resp = req.send().await?;
    Ok(json!({
        "status": resp.status().as_u16(),
        "content_type": resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("")
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn credentials_success_parses_fake_aws_response() {
        let value = credentials_result_from_stdout(
            r#"{"Account":"123456789012","Arn":"arn:aws:iam::123456789012:role/test","UserId":"abc"}"#,
            Some("emberlane-dev".to_string()),
            "us-west-2".to_string(),
        );
        assert_eq!(value["ok"], true);
        assert_eq!(value["account_id"], "123456789012");
        assert_eq!(value["profile"], "emberlane-dev");
    }

    #[test]
    fn credentials_failure_contains_setup_instructions() {
        let value = credentials_failure("no credentials");
        assert_eq!(value["ok"], false);
        assert!(value["message"].as_str().unwrap().contains("aws configure"));
        assert!(value["message"].as_str().unwrap().contains("aws sso login"));
    }

    #[test]
    fn aws_test_refuses_without_opt_in() {
        let err = ensure_aws_opt_in(false).unwrap_err();
        assert!(err.to_string().contains("creates billable AWS resources"));
    }

    #[test]
    fn model_split_supports_one_or_many_models() {
        assert_eq!(split_models("tiny_demo"), vec!["tiny_demo"]);
        assert_eq!(
            split_models("llama31_8b,qwen25_7b"),
            vec!["llama31_8b", "qwen25_7b"]
        );
    }

    #[test]
    fn matrix_parser_selects_cases() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("matrix.toml");
        fs::write(
            &path,
            r#"
[[cases]]
name = "a"
model = "tiny_demo"
accelerator = "cuda"
instance = "g4dn.xlarge"
mode = "economy"

[[cases]]
name = "b"
model = "llama32_1b_inf2"
accelerator = "inf2"
instance = "inf2.xlarge"
mode = "balanced"
experimental = true
"#,
        )
        .unwrap();
        let cases = load_matrix(&path).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(
            select_matrix_cases(cases.clone(), Some("a"), false).len(),
            1
        );
        assert_eq!(select_matrix_cases(cases, None, true).len(), 1);
    }

    #[tokio::test]
    async fn terraform_vars_include_unique_environment_and_test_run() {
        let mut backend = AwsBackend::load_or_default(Some(PathBuf::from("missing.toml"))).unwrap();
        backend.config.environment = "emberlane-it-tiny-demo-20260505".to_string();
        let vars = backend.render_deploy_vars().await.unwrap();
        assert_eq!(vars["environment"], "emberlane-it-tiny-demo-20260505");
        assert_eq!(
            vars["emberlane_test_run"],
            "emberlane-it-tiny-demo-20260505"
        );
    }

    #[test]
    fn cleanup_dry_run_lists_tagged_resource_policy() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let value = rt
            .block_on(cleanup_aws(CleanupOptions {
                environment: Some("emberlane-it-test".to_string()),
                test_run: None,
                force: false,
                dry_run: true,
                delete_bucket_contents: false,
            }))
            .unwrap();
        assert_eq!(value["dry_run"], true);
        assert!(
            value.to_string().contains("EmberlaneTestRun")
                || value.to_string().contains("Environment")
        );
    }

    #[test]
    fn report_writer_redacts_secrets() {
        let dir = TempDir::new().unwrap();
        let summary = json!({"api_key":"secret","presigned_url":"https://secret","ok":true});
        write_report(dir.path(), "report", &summary).unwrap();
        let text = fs::read_to_string(dir.path().join("report.json")).unwrap();
        assert!(!text.contains("https://secret"));
        assert!(!text.contains("secret"));
        assert!(text.contains("[redacted]"));
    }
}
