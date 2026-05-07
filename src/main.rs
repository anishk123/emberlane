mod cloud;
mod config;
mod error;
mod files;
mod mcp;
mod model;
mod provider;
mod router;
mod server;
mod storage;
mod test_harness;
mod util;

#[cfg(test)]
mod cloud_tests;
#[cfg(test)]
mod inf2_pack_tests;
#[cfg(test)]
mod terraform_pack_tests;

use clap::{Args, Parser, Subcommand};
use cloud::{model::CloudBackend, profiles, AwsBackend, CostMode};
use config::{EmberlaneConfig, S3StorageConfig};
use error::EmberlaneError;
use model::{ChatMessage, ChatRequest, RouteRequest, StorageBackend};
use provider::{aws_sample_config, parse_aws_asg_config, render_aws_iam_policy, RealCommandRunner};
use router::RuntimeRouter;
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf};
use storage::Storage;

#[derive(Parser)]
#[command(name = "emberlane", version, about = "Local-first wake gateway")]
struct Cli {
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init {
        #[arg(long)]
        force: bool,
    },
    Serve,
    Mcp,
    Status {
        runtime_id: Option<String>,
    },
    Wake {
        runtime_id: String,
    },
    Sleep {
        runtime_id: String,
    },
    Route(RouteCmd),
    Chat {
        runtime_id: String,
        message: String,
    },
    Upload {
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    ChatFile {
        runtime_id: String,
        file_id: String,
        message: String,
    },
    ChatFiles {
        runtime_id: String,
        #[arg(required = true)]
        file_ids: Vec<String>,
        #[arg(long)]
        message: String,
    },
    Aws {
        #[command(subcommand)]
        command: AwsCommand,
    },
    Storage {
        #[command(subcommand)]
        command: StorageCommand,
    },
    Files {
        #[command(subcommand)]
        command: FilesCommand,
    },
    Test {
        #[command(subcommand)]
        command: TestCommand,
    },
}

#[derive(Subcommand)]
enum AwsCommand {
    Init {
        #[arg(long)]
        force: bool,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    Credentials {
        #[command(subcommand)]
        command: AwsCredentialsCommand,
    },
    Doctor {
        runtime_id: Option<String>,
    },
    Deploy(AwsDeployCmd),
    Destroy {
        #[arg(long)]
        auto_approve: bool,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    Status {
        runtime_id: Option<String>,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    Chat {
        message: String,
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    Benchmark {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    CostReport {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    PrintConfig,
    SmokeTest,
    Diagnose {
        #[arg(long)]
        deployment: Option<PathBuf>,
    },
    Cleanup(AwsCleanupCmd),
    Models {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    Modes {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    #[command(hide = true)]
    Wake {
        runtime_id: String,
    },
    #[command(hide = true)]
    Sleep {
        runtime_id: String,
    },
    #[command(hide = true)]
    RenderIam {
        runtime_id: String,
    },
    #[command(hide = true)]
    SampleConfig {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    Login {
        #[arg(long)]
        profile: Option<String>,
    },
}

#[derive(Subcommand)]
enum AwsCredentialsCommand {
    Check {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
}

#[derive(Args)]
struct AwsDeployCmd {
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    accelerator: Option<String>,
    #[arg(long)]
    mode: Option<String>,
    #[arg(long)]
    instance: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    region: Option<String>,
    #[arg(long)]
    ami_id: Option<String>,
    #[arg(long)]
    auto_approve: bool,
    #[arg(long)]
    plan_only: bool,
    #[arg(long)]
    hf_token: Option<String>,
}

#[derive(Args)]
struct AwsCleanupCmd {
    #[arg(long)]
    environment: Option<String>,
    #[arg(long)]
    test_run: Option<PathBuf>,
    #[arg(long)]
    force: bool,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    delete_bucket_contents: bool,
}

#[derive(Subcommand)]
enum TestCommand {
    Local(LocalTestCmd),
    Aws(AwsTestCmd),
    AwsMatrix(AwsMatrixTestCmd),
}

#[derive(Args)]
struct LocalTestCmd {
    #[arg(long, default_value = "echo")]
    runtime: String,
    #[arg(long)]
    file: Option<PathBuf>,
    #[arg(long)]
    skip_mcp: bool,
    #[arg(long)]
    skip_http: bool,
    #[arg(long)]
    allow_missing_ollama: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Args, Clone)]
struct AwsTestCmd {
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    models: Option<String>,
    #[arg(long, default_value = "cuda")]
    accelerator: String,
    #[arg(long)]
    instance: Option<String>,
    #[arg(long, default_value = "economy")]
    mode: String,
    #[arg(long)]
    region: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    ami_id: Option<String>,
    #[arg(long)]
    destroy: bool,
    #[arg(long)]
    keep_on_failure: bool,
    #[arg(long)]
    auto_approve: bool,
    #[arg(long, default_value_t = 1800)]
    max_wait_secs: u64,
    #[arg(long)]
    skip_streaming: bool,
    #[arg(long)]
    skip_file: bool,
    #[arg(long)]
    skip_cost: bool,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    yes_i_understand_this_creates_aws_resources: bool,
}

#[derive(Args)]
struct AwsMatrixTestCmd {
    #[arg(long)]
    config: PathBuf,
    #[arg(long)]
    only: Option<String>,
    #[arg(long)]
    exclude_experimental: bool,
    #[arg(long)]
    destroy: bool,
    #[arg(long)]
    auto_approve: bool,
    #[arg(long)]
    parallel: bool,
    #[arg(long)]
    yes_i_understand_this_creates_aws_resources: bool,
}

#[derive(Subcommand)]
enum StorageCommand {
    Status {
        #[arg(long)]
        check: bool,
    },
    Use(StorageUseCmd),
}

#[derive(Args)]
struct StorageUseCmd {
    backend: String,
    #[arg(long)]
    bucket: Option<String>,
    #[arg(long, default_value = "uploads/")]
    prefix: String,
    #[arg(long)]
    region: Option<String>,
    #[arg(long)]
    profile: Option<String>,
    #[arg(long, default_value = "aws")]
    aws_cli: String,
    #[arg(long, default_value = "dev")]
    environment: String,
}

#[derive(Subcommand)]
enum FilesCommand {
    Get {
        file_id: String,
    },
    Presign {
        file_id: String,
        #[arg(long, default_value_t = 900)]
        expires: u64,
    },
    Route(FileRouteCmd),
}

#[derive(Args)]
struct RouteCmd {
    runtime_id: String,
    #[arg(long)]
    path: String,
    #[arg(long, default_value = "POST")]
    method: String,
    #[arg(long, default_value = "{}")]
    body: String,
}

#[derive(Args)]
struct FileRouteCmd {
    file_id: String,
    runtime_id: String,
    #[arg(long)]
    path: String,
    #[arg(long)]
    presign: bool,
    #[arg(long, default_value_t = 900)]
    expires: u64,
    #[arg(long, default_value = "{}")]
    body: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    if let Err(err) = run(Cli::parse()).await {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
    Ok(())
}

async fn run(cli: Cli) -> Result<(), EmberlaneError> {
    match cli.command {
        Command::Init { force } => {
            let path = cli
                .config
                .unwrap_or_else(|| PathBuf::from("emberlane.toml"));
            EmberlaneConfig::write_default(path.clone(), force)?;
            let cfg = EmberlaneConfig::discover(Some(path.clone()))?;
            std::fs::create_dir_all(cfg.files_dir())?;
            let storage = Storage::open(cfg.db_path())?;
            for runtime in &cfg.runtimes {
                storage.upsert_runtime(runtime)?;
            }
            println!("Initialized Emberlane at {}", path.display());
        }
        Command::Serve => {
            let router = local_router(cli.config)?;
            server::serve(router).await?;
        }
        Command::Mcp => {
            let router = local_router(cli.config)?;
            mcp::run_stdio(router).await?;
        }
        Command::Status { runtime_id } => {
            let router = local_router(cli.config)?;
            if let Some(runtime_id) = runtime_id {
                print_json(json!(router.status(&runtime_id).await?));
            } else {
                print_json(json!(router.list_status().await?));
            }
        }
        Command::Wake { runtime_id } => {
            let router = local_router(cli.config)?;
            router.wake(&runtime_id).await?;
            print_json(json!({"runtime_id": runtime_id, "state": "ready"}));
        }
        Command::Sleep { runtime_id } => {
            let router = local_router(cli.config)?;
            router.sleep(&runtime_id).await?;
            print_json(json!({"runtime_id": runtime_id, "state": "cold"}));
        }
        Command::Route(cmd) => {
            let router = local_router(cli.config)?;
            let body: Value = serde_json::from_str(&cmd.body)
                .map_err(|err| EmberlaneError::InvalidRequest(err.to_string()))?;
            let resp = router
                .route(
                    &cmd.runtime_id,
                    RouteRequest {
                        method: cmd.method,
                        path: cmd.path,
                        headers: HashMap::new(),
                        body,
                    },
                )
                .await?;
            print_json(resp.body);
        }
        Command::Chat {
            runtime_id,
            message,
        } => {
            let router = local_router(cli.config)?;
            let resp = router.chat(&runtime_id, chat_request(message)).await?;
            print_json(resp.body);
        }
        Command::Upload { paths } => {
            let router = local_router(cli.config)?;
            let mut records = Vec::new();
            for path in paths {
                records.push(router.upload_path(path).await?);
            }
            if records.len() == 1 {
                print_json(json!(&records[0]));
            } else {
                print_json(json!(records));
            }
        }
        Command::ChatFile {
            runtime_id,
            file_id,
            message,
        } => {
            let router = local_router(cli.config)?;
            let resp = router.chat_file(&runtime_id, &file_id, &message).await?;
            print_json(resp.body);
        }
        Command::ChatFiles {
            runtime_id,
            file_ids,
            message,
        } => {
            let router = local_router(cli.config)?;
            let resp = router.chat_files(&runtime_id, &file_ids, &message).await?;
            print_json(resp.body);
        }
        Command::Aws { command } => run_aws_command(cli.config, command).await?,
        Command::Storage { command } => run_storage_command(cli.config, command).await?,
        Command::Files { command } => run_files_command(cli.config, command).await?,
        Command::Test { command } => run_test_command(cli.config, command).await?,
    }
    Ok(())
}

async fn run_test_command(
    config: Option<PathBuf>,
    command: TestCommand,
) -> Result<(), EmberlaneError> {
    match command {
        TestCommand::Local(cmd) => {
            let report = test_harness::run_local_test(
                config,
                test_harness::LocalTestOptions {
                    runtime: cmd.runtime,
                    file: cmd.file,
                    skip_mcp: cmd.skip_mcp,
                    skip_http: cmd.skip_http,
                    allow_missing_ollama: cmd.allow_missing_ollama,
                },
            )
            .await?;
            if cmd.json {
                print_json(report.summary.clone());
            } else {
                println!("{}", report.markdown_path.display());
            }
        }
        TestCommand::Aws(cmd) => {
            let report = test_harness::run_aws_test(test_harness::AwsTestOptions {
                models: cmd
                    .models
                    .or(cmd.model)
                    .unwrap_or_else(|| "tiny_demo".to_string()),
                accelerator: cmd.accelerator,
                instance: cmd.instance,
                mode: cmd.mode,
                region: cmd.region,
                profile: cmd.profile,
                ami_id: cmd.ami_id,
                destroy: cmd.destroy,
                keep_on_failure: cmd.keep_on_failure,
                auto_approve: cmd.auto_approve,
                max_wait_secs: cmd.max_wait_secs,
                skip_streaming: cmd.skip_streaming,
                skip_file: cmd.skip_file,
                skip_cost: cmd.skip_cost,
                allow: cmd.yes_i_understand_this_creates_aws_resources,
            })
            .await?;
            if cmd.json {
                print_json(report.summary.clone());
            } else {
                println!("{}", report.markdown_path.display());
            }
        }
        TestCommand::AwsMatrix(cmd) => {
            if cmd.parallel {
                return Err(EmberlaneError::InvalidRequest(
                    "--parallel is not implemented yet; AWS matrix runs sequentially".to_string(),
                ));
            }
            let report = test_harness::run_aws_matrix_test(test_harness::AwsMatrixOptions {
                config: cmd.config,
                only: cmd.only,
                exclude_experimental: cmd.exclude_experimental,
                destroy: cmd.destroy,
                auto_approve: cmd.auto_approve,
                allow: cmd.yes_i_understand_this_creates_aws_resources,
            })
            .await?;
            print_json(report.summary);
        }
    }
    Ok(())
}

async fn run_storage_command(
    config: Option<PathBuf>,
    command: StorageCommand,
) -> Result<(), EmberlaneError> {
    match command {
        StorageCommand::Status { check } => {
            let cfg = EmberlaneConfig::discover(config)?;
            print_json(files::storage_status(&cfg, check).await?);
        }
        StorageCommand::Use(cmd) => {
            let mut cfg = EmberlaneConfig::discover(config)?;
            let backend = cmd.backend.parse::<StorageBackend>().map_err(|e| {
                EmberlaneError::InvalidRequest(format!("invalid storage backend: {e}"))
            })?;
            match backend {
                StorageBackend::Local => {
                    cfg.storage.backend = StorageBackend::Local;
                    cfg.storage.s3 = None;
                }
                StorageBackend::S3 => {
                    let aws_backend = AwsBackend::load_or_default(None)?;
                    let region = cmd
                        .region
                        .unwrap_or_else(|| aws_backend.config.region.clone());
                    let profile = cmd
                        .profile
                        .or_else(|| aws_backend.config.profile.clone())
                        .filter(|v| !v.trim().is_empty());
                    let bucket = if let Some(bucket) = cmd.bucket {
                        bucket
                    } else {
                        let credentials = test_harness::check_aws_credentials(
                            profile.clone(),
                            Some(region.clone()),
                        )
                        .await?;
                        if credentials["ok"].as_bool() != Some(true) {
                            return Err(EmberlaneError::ProviderNotConfigured(
                                credentials["message"]
                                    .as_str()
                                    .unwrap_or(
                                        "AWS credentials are required to derive the S3 bucket",
                                    )
                                    .to_string(),
                            ));
                        }
                        let account_id = credentials["account_id"]
                            .as_str()
                            .filter(|v| !v.trim().is_empty())
                            .ok_or_else(|| {
                                EmberlaneError::ProviderNotConfigured(
                                    "could not determine AWS account id for S3 bucket naming"
                                        .to_string(),
                                )
                            })?;
                        format!("emberlane-{}-{}-{}", cmd.environment, account_id, region)
                    };
                    cfg.storage.backend = StorageBackend::S3;
                    cfg.storage.s3 = Some(S3StorageConfig {
                        bucket,
                        prefix: cmd.prefix,
                        region,
                        aws_cli: cmd.aws_cli,
                        profile,
                        presign_downloads: true,
                        presign_expires_secs: 900,
                        pass_s3_uri: true,
                    });
                }
            }
            let path = cfg
                .config_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("emberlane.toml"));
            cfg.write_to(path.clone(), true)?;
            let mut storage_note = json!({"backend": cfg.storage.backend});
            if let Some(s3) = cfg.storage.s3.as_ref() {
                if let StorageBackend::S3 = cfg.storage.backend {
                    let created =
                        files::ensure_s3_bucket_exists(s3, std::sync::Arc::new(RealCommandRunner))
                            .await?;
                    storage_note["bucket"] = json!(s3.bucket);
                    storage_note["region"] = json!(s3.region);
                    storage_note["created_bucket"] = json!(created);
                }
            }
            print_json(json!({
                "ok": true,
                "config_path": path,
                "storage_note": storage_note,
                "storage": files::storage_status(&cfg, false).await?
            }));
        }
    }
    Ok(())
}

async fn run_files_command(
    config: Option<PathBuf>,
    command: FilesCommand,
) -> Result<(), EmberlaneError> {
    let router = local_router(config)?;
    match command {
        FilesCommand::Get { file_id } => {
            print_json(json!(router.file_metadata(&file_id)?));
        }
        FilesCommand::Presign { file_id, expires } => {
            let url = router.presign_file(&file_id, expires).await?;
            print_json(json!({"file_id": file_id, "url": url, "expires_secs": expires}));
        }
        FilesCommand::Route(cmd) => {
            let body: Value = serde_json::from_str(&cmd.body)
                .map_err(|err| EmberlaneError::InvalidRequest(err.to_string()))?;
            let resp = router
                .route_file(
                    &cmd.file_id,
                    &cmd.runtime_id,
                    &cmd.path,
                    cmd.presign,
                    cmd.expires,
                    body,
                )
                .await?;
            print_json(resp.body);
        }
    }
    Ok(())
}

async fn run_aws_command(
    config: Option<PathBuf>,
    command: AwsCommand,
) -> Result<(), EmberlaneError> {
    match command {
        AwsCommand::Init {
            force,
            profile,
            region,
        } => {
            let mut backend = AwsBackend::load_or_default(None)?;
            if let Some(profile) = profile {
                backend.config.profile = Some(profile);
            }
            if let Some(region) = region {
                backend.config.region = region;
            }
            print_json(backend.init_config(force).await?);
        }
        AwsCommand::Credentials {
            command: AwsCredentialsCommand::Check { profile, region },
        } => {
            let backend = AwsBackend::load_or_default(None)?;
            let profile = profile.or(backend.config.profile.clone());
            let region = region.or(Some(backend.config.region.clone()));
            print_json(test_harness::check_aws_credentials(profile, region).await?);
        }
        AwsCommand::Models { .. } => {
            print_json(json!({"models": profiles::rows()?}));
        }
        AwsCommand::Modes { .. } => {
            print_json(json!({
                "modes": CostMode::rows(),
                "caveat": "Benchmark results are workload and region dependent; Emberlane does not promise exact latency."
            }));
        }
        AwsCommand::PrintConfig => {
            let backend = AwsBackend::load_or_default(None)?;
            print_json(backend.render_deploy_vars().await?);
        }
        AwsCommand::Deploy(cmd) => {
            let config_profile = cmd.profile.clone().or_else(|| {
                AwsBackend::load_or_default(None)
                    .ok()
                    .and_then(|b| b.config.profile.clone())
            });
            let config_region = cmd.region.clone().unwrap_or_else(|| {
                AwsBackend::load_or_default(None)
                    .map(|b| b.config.region)
                    .unwrap_or_else(|_| "us-west-2".to_string())
            });

            ensure_aws_authenticated(config_profile.clone(), Some(config_region.clone())).await?;

            let mut model = cmd.model;
            let mut hf_token = cmd.hf_token;

            if model.is_none() {
                let profiles_res = profiles::all_profiles()?;
                let mut p_list = profiles_res.into_iter().collect::<Vec<_>>();
                p_list.sort_by_key(|(name, _)| name.clone());

                let prompts: Vec<String> = p_list
                    .iter()
                    .map(|(name, p)| format!("{} ({})", p.display_name, name))
                    .collect();

                let selection = dialoguer::Select::new()
                    .with_prompt("Select a model to deploy")
                    .items(&prompts)
                    .default(
                        p_list
                            .iter()
                            .position(|(name, _)| name == "qwen35_9b")
                            .unwrap_or(0),
                    )
                    .interact()
                    .map_err(|e| EmberlaneError::Internal(e.to_string()))?;

                let selected_name = p_list[selection].0.clone();
                let selected_id = p_list[selection].1.model_id.clone();
                model = Some(selected_name);

                let is_gated = selected_id.starts_with("meta-llama/")
                    || selected_id.starts_with("google/gemma-")
                    || selected_id.starts_with("mistralai/Mistral-")
                    || selected_id.starts_with("mistralai/Mixtral-");

                if is_gated && hf_token.is_none() {
                    let entered_token: String = dialoguer::Password::new()
                        .with_prompt("This is a gated model. Enter your Hugging Face token (it will be securely saved in AWS SSM)")
                        .interact()
                        .map_err(|e| EmberlaneError::Internal(e.to_string()))?;
                    if !entered_token.trim().is_empty() {
                        hf_token = Some(entered_token.trim().to_string());
                    }
                }
            }

            let mut backend = AwsBackend::load_or_default(None)?.with_overrides(
                model,
                cmd.accelerator,
                cmd.instance,
                cmd.mode,
                hf_token,
            )?;
            if let Some(profile) = cmd.profile {
                backend.config.profile = Some(profile);
            }
            if let Some(region) = cmd.region {
                backend.config.region = region;
            }
            if let Some(ami_id) = cmd.ami_id {
                backend.config.ami_id = ami_id;
            }
            print_json(backend.deploy(cmd.auto_approve, cmd.plan_only).await?);
        }
        AwsCommand::Destroy {
            auto_approve,
            profile,
            region,
        } => {
            let config_profile = profile.clone().or_else(|| {
                AwsBackend::load_or_default(None)
                    .ok()
                    .and_then(|b| b.config.profile.clone())
            });
            let config_region = region.clone().unwrap_or_else(|| {
                AwsBackend::load_or_default(None)
                    .map(|b| b.config.region)
                    .unwrap_or_else(|_| "us-west-2".to_string())
            });

            ensure_aws_authenticated(config_profile.clone(), Some(config_region.clone())).await?;

            let mut backend = AwsBackend::load_or_default(None)?;
            if let Some(profile) = profile {
                backend.config.profile = Some(profile);
            }
            if let Some(region) = region {
                backend.config.region = region;
            }
            print_json(backend.destroy(auto_approve).await?);
        }
        AwsCommand::Chat {
            message,
            profile,
            region,
        } => {
            let mut backend = AwsBackend::load_or_default(None)?;
            if let Some(profile) = profile {
                backend.config.profile = Some(profile);
            }
            if let Some(region) = region {
                backend.config.region = region;
            }
            print_json(backend.chat(&message).await?);
        }
        AwsCommand::Benchmark { profile, region } => {
            let mut backend = AwsBackend::load_or_default(None)?;
            if let Some(profile) = profile {
                backend.config.profile = Some(profile);
            }
            if let Some(region) = region {
                backend.config.region = region;
            }
            print_json(backend.benchmark().await?);
        }
        AwsCommand::CostReport { profile, region } => {
            let mut backend = AwsBackend::load_or_default(None)?;
            if let Some(profile) = profile {
                backend.config.profile = Some(profile);
            }
            if let Some(region) = region {
                backend.config.region = region;
            }
            print_json(backend.cost_report().await?);
        }
        AwsCommand::SmokeTest => {
            let backend = AwsBackend::load_or_default(None)?;
            let report = backend.benchmark().await?;
            print_json(json!({"smoke_test": report}));
        }
        AwsCommand::Diagnose { deployment } => {
            print_json(test_harness::diagnose_aws(deployment).await?);
        }
        AwsCommand::Cleanup(cmd) => {
            print_json(
                test_harness::cleanup_aws(test_harness::CleanupOptions {
                    environment: cmd.environment,
                    test_run: cmd.test_run,
                    force: cmd.force,
                    dry_run: cmd.dry_run,
                    delete_bucket_contents: cmd.delete_bucket_contents,
                })
                .await?,
            );
        }
        AwsCommand::SampleConfig { .. } => {
            println!("{}", aws_sample_config());
        }
        AwsCommand::RenderIam { runtime_id } => {
            let router = local_router(config)?;
            let runtime = router
                .storage
                .load_runtime(&runtime_id)?
                .ok_or_else(|| EmberlaneError::RuntimeNotFound(runtime_id.clone()))?;
            print_json(render_aws_iam_policy(&runtime)?);
        }
        AwsCommand::Status {
            runtime_id,
            profile,
            region,
        } => {
            if let Some(runtime_id) = runtime_id {
                let router = local_router(config)?;
                print_json(router.aws_status(&runtime_id).await?);
            } else {
                let mut backend = AwsBackend::load_or_default(None)?;
                if let Some(profile) = profile {
                    backend.config.profile = Some(profile);
                }
                if let Some(region) = region {
                    backend.config.region = region;
                }
                print_json(backend.status().await?);
            }
        }
        AwsCommand::Wake { runtime_id } => {
            let router = local_router(config)?;
            router.wake(&runtime_id).await?;
            let status = router.aws_status(&runtime_id).await.unwrap_or_else(|err| {
                json!({"runtime_id": runtime_id, "state": "ready", "status_warning": err.to_string()})
            });
            print_json(status);
        }
        AwsCommand::Sleep { runtime_id } => {
            let router = local_router(config)?;
            router.sleep(&runtime_id).await?;
            let status = router.aws_status(&runtime_id).await.unwrap_or_else(|err| {
                json!({"runtime_id": runtime_id, "state": "cold", "status_warning": err.to_string()})
            });
            print_json(status);
        }
        AwsCommand::Doctor { runtime_id } => {
            if let Some(runtime_id) = runtime_id {
                let router = local_router(config)?;
                let runtime = router
                    .storage
                    .load_runtime(&runtime_id)?
                    .ok_or_else(|| EmberlaneError::RuntimeNotFound(runtime_id.clone()))?;
                let cfg = parse_aws_asg_config(&runtime)?;
                let mut checks = Vec::new();
                checks.push(json!({"check": "runtime exists", "ok": true}));
                checks.push(json!({"check": "provider is aws_asg", "ok": true}));
                checks.push(json!({"check": "region configured", "ok": !cfg.region.is_empty(), "value": cfg.region}));
                checks.push(json!({"check": "base_url configured", "ok": runtime.base_url.is_some(), "value": runtime.base_url.clone()}));
                let cli_ok = tokio::process::Command::new(&cfg.aws_cli)
                    .arg("--version")
                    .output()
                    .await
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                checks.push(
                    json!({"check": "aws CLI available", "ok": cli_ok, "command": cfg.aws_cli}),
                );
                match router.aws_status(&runtime_id).await {
                    Ok(status) => {
                        checks.push(
                            json!({"check": "ASG describe succeeded", "ok": true, "status": status}),
                        );
                    }
                    Err(err) => {
                        checks.push(json!({"check": "ASG describe succeeded", "ok": false, "error": err.to_string()}));
                    }
                }
                match router.provider_health(&runtime_id).await {
                    Ok(healthy) => {
                        checks.push(json!({"check": "runtime health endpoint", "ok": healthy}));
                    }
                    Err(err) => {
                        checks.push(json!({"check": "runtime health endpoint", "ok": false, "error": err.to_string()}));
                    }
                }
                print_json(json!({"runtime_id": runtime_id, "checks": checks}));
            } else {
                let backend = AwsBackend::load_or_default(None)?;
                print_json(backend.doctor().await?);
            }
        }
        AwsCommand::Login { profile } => {
            let backend = AwsBackend::load_or_default(None)?;
            let profile = profile.or(backend.config.profile.clone());
            println!("[emberlane] Starting AWS login...");
            let mut cmd = tokio::process::Command::new("aws");
            cmd.arg("login"); // Use 'aws login' as discovered on this system
            if let Some(profile) = profile {
                cmd.arg("--profile").arg(profile);
            }
            let status = cmd.status().await.map_err(|e| {
                EmberlaneError::Internal(format!("Failed to execute 'aws login': {}", e))
            })?;
            if !status.success() {
                return Err(EmberlaneError::ProviderNotConfigured(
                    "AWS login failed.".to_string(),
                ));
            }
            println!("[emberlane] AWS login successful!");
        }
    }
    Ok(())
}

async fn ensure_aws_authenticated(
    profile: Option<String>,
    region: Option<String>,
) -> Result<(), EmberlaneError> {
    let sts = test_harness::check_aws_credentials(profile.clone(), region.clone()).await?;
    if sts["ok"].as_bool() == Some(true) {
        return Ok(());
    }

    println!("[emberlane] AWS credentials expired or missing. Attempting login...");

    let profile_arg = profile.clone().unwrap_or_else(|| "default".to_string());

    // Try 'aws login' first (as discovered on this system)
    let mut cmd = tokio::process::Command::new("aws");
    cmd.args(["login", "--profile", &profile_arg]);

    let status = cmd
        .status()
        .await
        .map_err(|e| EmberlaneError::Internal(format!("Failed to run 'aws login': {}", e)))?;

    if status.success() {
        println!("[emberlane] AWS login successful!");
        return Ok(());
    }

    // Fallback to 'aws sso login' if 'aws login' fails
    println!("[emberlane] 'aws login' failed. Trying 'aws sso login'...");
    let mut sso_cmd = tokio::process::Command::new("aws");
    sso_cmd.args(["sso", "login", "--profile", &profile_arg]);

    let output = sso_cmd
        .output()
        .await
        .map_err(|e| EmberlaneError::Internal(format!("Failed to run 'aws sso login': {}", e)))?;

    if output.status.success() {
        println!("[emberlane] AWS login successful!");
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("Missing the following required SSO configuration values") {
        println!(
            "\n[emberlane] It looks like your AWS profile '{}' is not fully configured for SSO.",
            profile_arg
        );
        println!("[emberlane] Please enter your SSO configuration one time:");

        let start_url: String = dialoguer::Input::new()
            .with_prompt("SSO Start URL (e.g., https://my-company.awsapps.com/start)")
            .interact()
            .map_err(|e| EmberlaneError::Internal(e.to_string()))?;

        let sso_region: String = dialoguer::Input::new()
            .with_prompt("SSO Region (e.g., us-west-2)")
            .default("us-west-2".to_string())
            .interact()
            .map_err(|e| EmberlaneError::Internal(e.to_string()))?;

        // Configure the profile
        println!("[emberlane] Configuring AWS profile '{}'...", profile_arg);
        tokio::process::Command::new("aws")
            .args([
                "configure",
                "set",
                "sso_start_url",
                &start_url,
                "--profile",
                &profile_arg,
            ])
            .status()
            .await
            .ok();
        tokio::process::Command::new("aws")
            .args([
                "configure",
                "set",
                "sso_region",
                &sso_region,
                "--profile",
                &profile_arg,
            ])
            .status()
            .await
            .ok();

        // Try login again
        println!("[emberlane] Retrying login...");
        let status = tokio::process::Command::new("aws")
            .args(["sso", "login", "--profile", &profile_arg])
            .status()
            .await
            .map_err(|e| {
                EmberlaneError::Internal(format!(
                    "Failed to run 'aws sso login' after config: {}",
                    e
                ))
            })?;

        if status.success() {
            println!("[emberlane] AWS login successful!");
            return Ok(());
        }
    }

    Err(EmberlaneError::ProviderNotConfigured(format!(
        "AWS login failed. Status code: {}. Stderr: {}",
        output.status.code().unwrap_or(1),
        stderr
    )))
}

fn local_router(config: Option<PathBuf>) -> Result<RuntimeRouter, EmberlaneError> {
    let cfg = EmberlaneConfig::discover(config)?;
    std::fs::create_dir_all(cfg.files_dir())?;
    let storage = Storage::open(cfg.db_path())?;
    let router = RuntimeRouter::new(cfg, storage);
    router.seed_config_runtimes()?;
    Ok(router)
}

fn chat_request(message: String) -> ChatRequest {
    ChatRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: message,
        }],
        files: vec![],
    }
}

fn print_json(value: Value) {
    println!("{}", serde_json::to_string_pretty(&value).unwrap());
}
