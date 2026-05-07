use crate::{
    config::EmberlaneConfig,
    error::EmberlaneError,
    files::{self, ArtifactStore},
    model::{
        ChatMessage, ChatRequest, RouteRequest, RouteResponse, RuntimeConfig, RuntimeMode,
        RuntimeStateKind, RuntimeStatus, StorageBackend, WarmingResponse,
    },
    provider::{CommandRunner, ProviderRegistry, RealCommandRunner},
    storage::Storage,
};
use serde_json::{json, Value};
use std::{collections::HashMap, path::Path, sync::Arc};
use tokio::{
    sync::{Mutex, Semaphore},
    time::{timeout, Duration},
};

#[derive(Clone)]
pub struct RuntimeRouter {
    pub cfg: Arc<EmberlaneConfig>,
    pub storage: Storage,
    providers: ProviderRegistry,
    command_runner: Arc<dyn CommandRunner>,
    wake_locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    semaphores: Arc<Mutex<HashMap<String, Arc<Semaphore>>>>,
}

impl RuntimeRouter {
    pub fn new(cfg: EmberlaneConfig, storage: Storage) -> Self {
        Self::with_providers_and_command_runner(
            cfg,
            storage,
            ProviderRegistry::new(),
            Arc::new(RealCommandRunner),
        )
    }

    #[allow(dead_code)]
    pub fn with_providers(
        cfg: EmberlaneConfig,
        storage: Storage,
        providers: ProviderRegistry,
    ) -> Self {
        Self::with_providers_and_command_runner(
            cfg,
            storage,
            providers,
            Arc::new(RealCommandRunner),
        )
    }

    pub fn with_providers_and_command_runner(
        cfg: EmberlaneConfig,
        storage: Storage,
        providers: ProviderRegistry,
        command_runner: Arc<dyn CommandRunner>,
    ) -> Self {
        Self {
            cfg: Arc::new(cfg),
            storage,
            providers,
            command_runner,
            wake_locks: Arc::new(Mutex::new(HashMap::new())),
            semaphores: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn seed_config_runtimes(&self) -> Result<(), EmberlaneError> {
        for runtime in &self.cfg.runtimes {
            self.storage.upsert_runtime(runtime)?;
        }
        Ok(())
    }

    pub fn list_runtimes(&self) -> Result<Vec<RuntimeConfig>, EmberlaneError> {
        self.storage.list_runtimes()
    }

    pub async fn status(&self, runtime_id: &str) -> Result<RuntimeStatus, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        let state = self.storage.get_runtime_state(runtime_id)?;
        let provider_status = if runtime.provider == crate::model::ProviderKind::AwsAsg {
            Some(self.providers.aws_status(&runtime).await?)
        } else {
            None
        };
        Ok(RuntimeStatus {
            runtime,
            state,
            provider_status,
        })
    }

    pub async fn aws_status(&self, runtime_id: &str) -> Result<Value, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        if runtime.provider != crate::model::ProviderKind::AwsAsg {
            return Err(EmberlaneError::InvalidRequest(format!(
                "runtime '{}' uses provider '{}', not aws_asg",
                runtime.id, runtime.provider
            )));
        }
        self.providers.aws_status(&runtime).await
    }

    pub async fn provider_health(&self, runtime_id: &str) -> Result<bool, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        let provider = self.providers.provider(&runtime.provider);
        provider.health(&runtime).await
    }

    pub async fn list_status(&self) -> Result<Vec<RuntimeStatus>, EmberlaneError> {
        self.storage.list_runtime_status()
    }

    pub async fn wake(&self, runtime_id: &str) -> Result<(), EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        self.storage.record_event(
            Some(runtime_id),
            "wake_requested",
            Some("wake requested"),
            None,
        )?;
        self.wake_runtime(&runtime).await
    }

    pub async fn sleep(&self, runtime_id: &str) -> Result<(), EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        self.storage.record_event(
            Some(runtime_id),
            "sleep_requested",
            Some("sleep requested"),
            None,
        )?;
        self.storage
            .set_runtime_state(runtime_id, RuntimeStateKind::Sleeping, None)?;
        let provider = self.providers.provider(&runtime.provider);
        provider.sleep(&runtime).await?;
        self.storage
            .set_runtime_state(runtime_id, RuntimeStateKind::Cold, None)?;
        Ok(())
    }

    pub async fn route(
        &self,
        runtime_id: &str,
        mut request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;

        // Inject API Key if configured
        if let Some(key) = self.cfg.api_key() {
            if !request
                .headers
                .iter()
                .any(|(k, _)| k.to_lowercase() == "authorization")
            {
                request
                    .headers
                    .insert("Authorization".to_string(), format!("Bearer {}", key));
            }
        }

        self.ensure_ready(&runtime).await?;
        self.with_concurrency(&runtime, request).await
    }

    pub async fn chat(
        &self,
        runtime_id: &str,
        request: ChatRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        self.route(
            runtime_id,
            RouteRequest {
                method: "POST".to_string(),
                path: "/chat".to_string(),
                headers: HashMap::new(),
                body: serde_json::to_value(request).unwrap(),
            },
        )
        .await
    }

    pub async fn chat_file(
        &self,
        runtime_id: &str,
        file_id: &str,
        message: &str,
    ) -> Result<RouteResponse, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        let file = self.storage.get_file(file_id)?;
        if file.storage_backend == StorageBackend::Local
            || file.size_bytes as u64 <= self.cfg.storage.inline_file_max_bytes
        {
            ensure_supported_text_file(&file.original_name)?;
            let content = String::from_utf8(self.artifact_store()?.get_file_bytes(&file).await?)
                .map_err(|err| {
                    EmberlaneError::InvalidRequest(format!("stored file is not UTF-8 text: {err}"))
                })?;
            let prompt = format!(
                "Use the following file content to answer the user question.\nFile name: {}\nFile content:\n{}\nUser question: {}",
                file.original_name, content, message
            );
            return self
                .chat(
                    runtime_id,
                    ChatRequest {
                        messages: vec![ChatMessage {
                            role: "user".to_string(),
                            content: prompt,
                        }],
                        files: vec![file_id.to_string()],
                    },
                )
                .await;
        }
        if runtime.provider == crate::model::ProviderKind::AwsAsg
            && file.storage_backend == StorageBackend::S3
        {
            return self
                .route_file_with_runtime(
                    &runtime,
                    &file,
                    "/chat",
                    self.cfg
                        .storage
                        .s3
                        .as_ref()
                        .map(|s3| s3.presign_downloads)
                        .unwrap_or(false),
                    self.cfg
                        .storage
                        .s3
                        .as_ref()
                        .map(|s3| s3.presign_expires_secs)
                        .unwrap_or(900),
                    json!({"messages": [{"role": "user", "content": message}], "files": [file_id]}),
                )
                .await;
        }
        ensure_supported_text_file(&file.original_name)?;
        Err(EmberlaneError::InvalidRequest(
            "S3 file is larger than inline_file_max_bytes for local file-context chat".to_string(),
        ))
    }

    pub async fn upload_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<crate::model::FileRecord, EmberlaneError> {
        let path = path.as_ref();
        let name = crate::util::safe_file_name(path);
        let mime = mime_guess::from_path(path)
            .first()
            .map(|m| m.essence_str().to_string());
        let record = self.artifact_store()?.put_file(path, &name, mime).await?;
        self.storage.insert_file(&record)?;
        Ok(record)
    }

    pub async fn upload_bytes(
        &self,
        original_name: &str,
        bytes: &[u8],
    ) -> Result<crate::model::FileRecord, EmberlaneError> {
        let tmp_dir = self.cfg.server.data_dir.join("tmp");
        std::fs::create_dir_all(&tmp_dir)?;
        let tmp_path = tmp_dir.join(format!("upload-{}", crate::util::uuid()));
        std::fs::write(&tmp_path, bytes)?;
        let mime = mime_guess::from_path(original_name)
            .first()
            .map(|m| m.essence_str().to_string());
        let result = self
            .artifact_store()?
            .put_file(&tmp_path, original_name, mime)
            .await;
        let _ = std::fs::remove_file(&tmp_path);
        let record = result?;
        self.storage.insert_file(&record)?;
        Ok(record)
    }

    pub fn file_metadata(&self, file_id: &str) -> Result<crate::model::FileRecord, EmberlaneError> {
        self.storage.get_file(file_id)
    }

    pub async fn presign_file(
        &self,
        file_id: &str,
        expires_secs: u64,
    ) -> Result<String, EmberlaneError> {
        let file = self.storage.get_file(file_id)?;
        if file.storage_backend != StorageBackend::S3 {
            return Err(EmberlaneError::PresignNotSupported);
        }
        self.artifact_store()?
            .presign_get(&file, expires_secs)
            .await?
            .ok_or(EmberlaneError::PresignNotSupported)
    }

    pub async fn route_file(
        &self,
        file_id: &str,
        runtime_id: &str,
        path: &str,
        include_presigned_url: bool,
        expires_secs: u64,
        body: Value,
    ) -> Result<RouteResponse, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        let file = self.storage.get_file(file_id)?;
        self.route_file_with_runtime(
            &runtime,
            &file,
            path,
            include_presigned_url,
            expires_secs,
            body,
        )
        .await
    }

    pub async fn openai_chat(
        &self,
        runtime_id: Option<&str>,
        body: Value,
    ) -> Result<RouteResponse, EmberlaneError> {
        if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
            return Err(EmberlaneError::InvalidRequest(
                "streaming is not implemented in Emberlane v0.1".to_string(),
            ));
        }
        let runtime_id = runtime_id
            .map(ToOwned::to_owned)
            .or_else(|| {
                body.get("model").and_then(Value::as_str).map(|model| {
                    if self.storage.load_runtime(model).ok().flatten().is_some() {
                        model.to_string()
                    } else if model == "echo" {
                        "echo".to_string()
                    } else if model == "ollama" || model == self.cfg.server.default_ollama_model {
                        "ollama".to_string()
                    } else {
                        self.cfg.server.default_runtime_id.clone()
                    }
                })
            })
            .unwrap_or_else(|| self.cfg.server.default_runtime_id.clone());
        self.route(
            &runtime_id,
            RouteRequest {
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                headers: HashMap::new(),
                body,
            },
        )
        .await
    }

    pub fn warming_body(&self, runtime_id: &str) -> Result<WarmingResponse, EmberlaneError> {
        let runtime = self.runtime(runtime_id)?;
        Ok(WarmingResponse {
            runtime_id: runtime_id.to_string(),
            state: "waking".to_string(),
            mode: runtime.mode,
            message: "Runtime is warming".to_string(),
            retry_after_secs: runtime.slow_retry_after_secs,
        })
    }

    fn runtime(&self, runtime_id: &str) -> Result<RuntimeConfig, EmberlaneError> {
        let runtime = self
            .storage
            .load_runtime(runtime_id)?
            .ok_or_else(|| EmberlaneError::RuntimeNotFound(runtime_id.to_string()))?;
        if !runtime.enabled {
            return Err(EmberlaneError::RuntimeDisabled(runtime_id.to_string()));
        }
        Ok(runtime)
    }

    fn artifact_store(&self) -> Result<Arc<dyn ArtifactStore>, EmberlaneError> {
        files::artifact_store_with_runner(&self.cfg, self.command_runner.clone())
    }

    async fn route_file_with_runtime(
        &self,
        runtime: &RuntimeConfig,
        file: &crate::model::FileRecord,
        path: &str,
        include_presigned_url: bool,
        expires_secs: u64,
        mut body: Value,
    ) -> Result<RouteResponse, EmberlaneError> {
        if runtime.provider == crate::model::ProviderKind::AwsAsg
            && file.storage_backend == StorageBackend::Local
        {
            return Err(EmberlaneError::LocalFileNotAvailableToRemoteRuntime);
        }
        let presigned_url = if include_presigned_url && file.storage_backend == StorageBackend::S3 {
            self.artifact_store()?
                .presign_get(file, expires_secs)
                .await?
        } else {
            None
        };
        let include_local_path = file.storage_backend == StorageBackend::Local
            && runtime.provider != crate::model::ProviderKind::AwsAsg;
        if !body.is_object() {
            return Err(EmberlaneError::InvalidRequest(
                "file route body must be a JSON object".to_string(),
            ));
        }
        body["file"] = files::file_reference(file, presigned_url, include_local_path);
        self.route(
            &runtime.id,
            RouteRequest {
                method: "POST".to_string(),
                path: path.to_string(),
                headers: HashMap::new(),
                body,
            },
        )
        .await
    }

    async fn ensure_ready(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        let provider = self.providers.provider(&runtime.provider);
        if provider.health(runtime).await? {
            self.storage
                .set_runtime_state(&runtime.id, RuntimeStateKind::Ready, None)?;
            return Ok(());
        }

        match runtime.mode {
            RuntimeMode::Fast => {
                let lock = self.wake_lock(&runtime.id).await;
                let _guard = lock.lock().await;
                if provider.health(runtime).await? {
                    self.storage
                        .set_runtime_state(&runtime.id, RuntimeStateKind::Ready, None)?;
                    return Ok(());
                }
                self.storage
                    .set_runtime_state(&runtime.id, RuntimeStateKind::Waking, None)?;
                let result = timeout(
                    Duration::from_secs(runtime.fast_wait_secs),
                    provider.wake(runtime),
                )
                .await;
                match result {
                    Ok(Ok(())) => {
                        self.storage.set_runtime_state(
                            &runtime.id,
                            RuntimeStateKind::Ready,
                            None,
                        )?;
                        Ok(())
                    }
                    Ok(Err(err)) => {
                        self.storage.set_runtime_state(
                            &runtime.id,
                            RuntimeStateKind::Failed,
                            Some(err.to_string()),
                        )?;
                        Err(err)
                    }
                    Err(_) => Err(EmberlaneError::RuntimeWarming(runtime.id.clone())),
                }
            }
            RuntimeMode::Slow => {
                let lock = self.wake_lock(&runtime.id).await;
                if let Ok(guard) = lock.clone().try_lock_owned() {
                    let provider = provider.clone();
                    let storage = self.storage.clone();
                    let runtime = runtime.clone();
                    tokio::spawn(async move {
                        let _guard = guard;
                        let _ =
                            storage.set_runtime_state(&runtime.id, RuntimeStateKind::Waking, None);
                        match provider.wake(&runtime).await {
                            Ok(()) => {
                                let _ = storage.set_runtime_state(
                                    &runtime.id,
                                    RuntimeStateKind::Ready,
                                    None,
                                );
                            }
                            Err(err) => {
                                let _ = storage.set_runtime_state(
                                    &runtime.id,
                                    RuntimeStateKind::Failed,
                                    Some(err.to_string()),
                                );
                            }
                        }
                    });
                }
                Err(EmberlaneError::RuntimeWarming(runtime.id.clone()))
            }
        }
    }

    async fn wake_runtime(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        let lock = self.wake_lock(&runtime.id).await;
        let _guard = lock.lock().await;
        let provider = self.providers.provider(&runtime.provider);
        self.storage
            .set_runtime_state(&runtime.id, RuntimeStateKind::Waking, None)?;
        provider.wake(runtime).await?;
        self.storage
            .set_runtime_state(&runtime.id, RuntimeStateKind::Ready, None)?;
        Ok(())
    }

    async fn with_concurrency(
        &self,
        runtime: &RuntimeConfig,
        request: RouteRequest,
    ) -> Result<RouteResponse, EmberlaneError> {
        let _permit = if let Some(max) = runtime.max_concurrency {
            let semaphore = self.semaphore(&runtime.id, max).await;
            Some(
                semaphore
                    .try_acquire_owned()
                    .map_err(|_| EmberlaneError::MaxConcurrencyExceeded)?,
            )
        } else {
            None
        };
        self.storage.increment_in_flight(&runtime.id)?;
        self.storage.record_event(
            Some(&runtime.id),
            "route_started",
            Some(&request.path),
            None,
        )?;
        let provider = self.providers.provider(&runtime.provider);
        let result = provider.route(runtime, request).await;
        self.storage.decrement_in_flight(&runtime.id)?;
        match result {
            Ok(resp) => {
                self.storage.record_event(
                    Some(&runtime.id),
                    "route_completed",
                    Some("route completed"),
                    Some(json!({"status": resp.status})),
                )?;
                Ok(resp)
            }
            Err(err) => {
                self.storage.record_event(
                    Some(&runtime.id),
                    "route_failed",
                    Some(&err.to_string()),
                    None,
                )?;
                Err(err)
            }
        }
    }

    async fn wake_lock(&self, runtime_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.wake_locks.lock().await;
        locks
            .entry(runtime_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn semaphore(&self, runtime_id: &str, max: u32) -> Arc<Semaphore> {
        let mut semaphores = self.semaphores.lock().await;
        semaphores
            .entry(runtime_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(max as usize)))
            .clone()
    }
}

fn ensure_supported_text_file(name: &str) -> Result<(), EmberlaneError> {
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext == "txt" || ext == "md" {
        Ok(())
    } else {
        Err(EmberlaneError::InvalidRequest(
            "file-context chat supports only .txt and .md files in v0.1".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{default_echo_runtime, S3StorageConfig},
        model::{FileRecord, ProviderKind},
        provider::{CommandOutput, CommandRunner, ProviderRegistry},
        storage::Storage,
        util,
    };
    use async_trait::async_trait;
    use axum::{http::StatusCode, routing::post, Json, Router};
    use std::{
        collections::VecDeque,
        fs,
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
        async fn push(&self, output: CommandOutput) {
            self.outputs.lock().await.push_back(output);
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

    fn mock_router(mode: RuntimeMode) -> RuntimeRouter {
        let mut cfg = EmberlaneConfig::default();
        let mut rt = default_echo_runtime();
        rt.provider = ProviderKind::Mock;
        rt.mode = mode;
        rt.config = json!({"wake_delay_ms": 10});
        rt.max_concurrency = Some(1);
        cfg.runtimes = vec![rt];
        let storage = Storage::open_memory().unwrap();
        let router = RuntimeRouter::new(cfg, storage);
        router.seed_config_runtimes().unwrap();
        router
    }

    fn aws_router(base_url: String, mode: RuntimeMode) -> (RuntimeRouter, Arc<FakeCommandRunner>) {
        let mut cfg = EmberlaneConfig::default();
        let mut rt = default_echo_runtime();
        rt.id = "aws-echo".to_string();
        rt.name = "AWS Echo Runtime".to_string();
        rt.provider = ProviderKind::AwsAsg;
        rt.mode = mode;
        rt.base_url = Some(base_url);
        rt.startup_timeout_secs = 1;
        rt.fast_wait_secs = 1;
        rt.config = json!({
            "region": "us-west-2",
            "asg_name": "emberlane-echo-asg",
            "desired_capacity_on_wake": 1,
            "desired_capacity_on_sleep": 0
        });
        cfg.runtimes = vec![rt];
        let storage = Storage::open_memory().unwrap();
        let runner = Arc::new(FakeCommandRunner::default());
        let providers = ProviderRegistry::with_command_runner(runner.clone());
        let router = RuntimeRouter::with_providers_and_command_runner(
            cfg,
            storage,
            providers,
            runner.clone(),
        );
        router.seed_config_runtimes().unwrap();
        (router, runner)
    }

    fn s3_router() -> (RuntimeRouter, Arc<FakeCommandRunner>) {
        let mut cfg = EmberlaneConfig::default();
        cfg.storage.backend = StorageBackend::S3;
        cfg.storage.s3 = Some(S3StorageConfig {
            bucket: "bucket".to_string(),
            prefix: "uploads/".to_string(),
            region: "us-west-2".to_string(),
            aws_cli: "aws".to_string(),
            profile: None,
            presign_downloads: true,
            presign_expires_secs: 900,
            pass_s3_uri: true,
        });
        cfg.runtimes[0].provider = ProviderKind::Mock;
        cfg.runtimes[0].config = json!({});
        cfg.runtimes.truncate(1);
        let storage = Storage::open_memory().unwrap();
        let runner = Arc::new(FakeCommandRunner::default());
        let providers = ProviderRegistry::with_command_runner(runner.clone());
        let router = RuntimeRouter::with_providers_and_command_runner(
            cfg,
            storage,
            providers,
            runner.clone(),
        );
        router.seed_config_runtimes().unwrap();
        (router, runner)
    }

    fn s3_file(size_bytes: i64) -> FileRecord {
        FileRecord {
            id: "s3-file".to_string(),
            original_name: "note.md".to_string(),
            stored_path: None,
            storage_backend: StorageBackend::S3,
            storage_key: Some("uploads/2026/05/04/s3-file/note.md".to_string()),
            bucket: Some("bucket".to_string()),
            region: Some("us-west-2".to_string()),
            s3_uri: Some("s3://bucket/uploads/2026/05/04/s3-file/note.md".to_string()),
            mime_type: Some("text/markdown".to_string()),
            size_bytes,
            sha256: Some("x".to_string()),
            created_at: util::now(),
        }
    }

    #[tokio::test]
    async fn fast_mode_auto_wakes_and_chat_proxies() {
        let router = mock_router(RuntimeMode::Fast);
        let resp = router
            .chat(
                "echo",
                ChatRequest {
                    messages: vec![ChatMessage {
                        role: "user".to_string(),
                        content: "hello".to_string(),
                    }],
                    files: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.body["reply"], "Echo: hello");
    }

    #[tokio::test]
    async fn route_auto_wakes() {
        let router = mock_router(RuntimeMode::Fast);
        let resp = router
            .route(
                "echo",
                RouteRequest {
                    method: "POST".to_string(),
                    path: "/echo".to_string(),
                    headers: HashMap::new(),
                    body: json!({"ok": true}),
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn slow_mode_returns_warming() {
        let router = mock_router(RuntimeMode::Slow);
        let err = router
            .route(
                "echo",
                RouteRequest {
                    method: "POST".to_string(),
                    path: "/echo".to_string(),
                    headers: HashMap::new(),
                    body: json!({}),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, EmberlaneError::RuntimeWarming(_)));
    }

    #[tokio::test]
    async fn max_concurrency_exceeded_works() {
        let router = mock_router(RuntimeMode::Fast);
        let runtime = router.runtime("echo").unwrap();
        let permit = router
            .semaphore("echo", 1)
            .await
            .try_acquire_owned()
            .unwrap();
        let err = router
            .with_concurrency(
                &runtime,
                RouteRequest {
                    method: "POST".to_string(),
                    path: "/echo".to_string(),
                    headers: HashMap::new(),
                    body: json!({}),
                },
            )
            .await
            .unwrap_err();
        drop(permit);
        assert!(matches!(err, EmberlaneError::MaxConcurrencyExceeded));
    }

    #[tokio::test]
    async fn chat_file_rejects_unsupported_extension() {
        let router = mock_router(RuntimeMode::Fast);
        let file = crate::model::FileRecord {
            id: "f".to_string(),
            original_name: "image.png".to_string(),
            stored_path: Some("/tmp/nope".to_string()),
            storage_backend: StorageBackend::Local,
            storage_key: None,
            bucket: None,
            region: None,
            s3_uri: None,
            mime_type: None,
            size_bytes: 1,
            sha256: Some("x".to_string()),
            created_at: util::now(),
        };
        router.storage.insert_file(&file).unwrap();
        assert!(router.chat_file("echo", "f", "summarize").await.is_err());
    }

    #[tokio::test]
    async fn chat_file_accepts_markdown_and_sends_content() {
        let router = mock_router(RuntimeMode::Fast);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "emberlane notes").unwrap();
        let file = crate::model::FileRecord {
            id: "f".to_string(),
            original_name: "note.md".to_string(),
            stored_path: Some(path.to_string_lossy().to_string()),
            storage_backend: StorageBackend::Local,
            storage_key: None,
            bucket: None,
            region: None,
            s3_uri: None,
            mime_type: Some("text/markdown".to_string()),
            size_bytes: 15,
            sha256: Some("x".to_string()),
            created_at: util::now(),
        };
        router.storage.insert_file(&file).unwrap();
        let resp = router.chat_file("echo", "f", "summarize").await.unwrap();
        assert!(resp.body["reply"]
            .as_str()
            .unwrap()
            .contains("emberlane notes"));
    }

    #[tokio::test]
    async fn chat_file_for_s3_small_markdown_downloads_and_inlines_content() {
        let (router, runner) = s3_router();
        runner
            .push(CommandOutput {
                status: 0,
                stdout: "s3 markdown notes".to_string(),
                stderr: String::new(),
            })
            .await;
        router.storage.insert_file(&s3_file(17)).unwrap();
        let resp = router
            .chat_file("echo", "s3-file", "summarize")
            .await
            .unwrap();
        assert!(resp.body["reply"]
            .as_str()
            .unwrap()
            .contains("s3 markdown notes"));
        assert!(runner.calls.lock().await[0].1.contains(&"cp".to_string()));
    }

    #[tokio::test]
    async fn route_file_for_s3_includes_uri_and_presigned_url() {
        let (router, runner) = s3_router();
        runner
            .push(CommandOutput {
                status: 0,
                stdout: "https://example.test/presigned\n".to_string(),
                stderr: String::new(),
            })
            .await;
        router.storage.insert_file(&s3_file(300_000)).unwrap();
        let resp = router
            .route_file(
                "s3-file",
                "echo",
                "/process-file",
                true,
                900,
                json!({"task": "process"}),
            )
            .await
            .unwrap();
        assert_eq!(resp.body["body"]["task"], "process");
        assert_eq!(
            resp.body["body"]["file"]["s3_uri"],
            "s3://bucket/uploads/2026/05/04/s3-file/note.md"
        );
        assert_eq!(
            resp.body["body"]["file"]["presigned_url"],
            "https://example.test/presigned"
        );
        assert!(resp.body["body"]["file"].get("stored_path").is_none());
    }

    #[tokio::test]
    async fn route_file_from_local_to_aws_asg_is_blocked() {
        let (router, _runner) = aws_router("http://127.0.0.1:1".to_string(), RuntimeMode::Fast);
        let file = FileRecord {
            id: "local-file".to_string(),
            original_name: "note.md".to_string(),
            stored_path: Some("/tmp/note.md".to_string()),
            storage_backend: StorageBackend::Local,
            storage_key: None,
            bucket: None,
            region: None,
            s3_uri: None,
            mime_type: Some("text/markdown".to_string()),
            size_bytes: 10,
            sha256: Some("x".to_string()),
            created_at: util::now(),
        };
        router.storage.insert_file(&file).unwrap();
        let err = router
            .route_file(
                "local-file",
                "aws-echo",
                "/process-file",
                false,
                900,
                json!({}),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            EmberlaneError::LocalFileNotAvailableToRemoteRuntime
        ));
    }

    #[tokio::test]
    async fn fast_mode_with_aws_asg_auto_wakes_and_proxies() {
        let hits = Arc::new(AtomicUsize::new(0));
        let health_hits = hits.clone();
        let app = Router::new()
            .route(
                "/health",
                axum::routing::get(move || {
                    let health_hits = health_hits.clone();
                    async move {
                        if health_hits.fetch_add(1, Ordering::SeqCst) < 3 {
                            StatusCode::SERVICE_UNAVAILABLE
                        } else {
                            StatusCode::OK
                        }
                    }
                }),
            )
            .route("/chat", post(|Json(body): Json<Value>| async move {
                Json(json!({"reply": format!("AWS Echo: {}", body["messages"][0]["content"].as_str().unwrap_or(""))}))
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let (router, runner) = aws_router(format!("http://{addr}"), RuntimeMode::Fast);
        let resp = router
            .chat(
                "aws-echo",
                ChatRequest {
                    messages: vec![ChatMessage {
                        role: "user".to_string(),
                        content: "hello".to_string(),
                    }],
                    files: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(resp.body["reply"], "AWS Echo: hello");
        let calls = runner.calls.lock().await;
        assert!(calls
            .iter()
            .any(|(_, args)| args.contains(&"set-desired-capacity".to_string())));
    }

    #[tokio::test]
    async fn slow_mode_with_aws_asg_returns_warming() {
        let app = Router::new().route(
            "/health",
            axum::routing::get(|| async { StatusCode::SERVICE_UNAVAILABLE }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let (router, _runner) = aws_router(format!("http://{addr}"), RuntimeMode::Slow);
        let err = router
            .route(
                "aws-echo",
                RouteRequest {
                    method: "POST".to_string(),
                    path: "/chat".to_string(),
                    headers: HashMap::new(),
                    body: json!({}),
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, EmberlaneError::RuntimeWarming(_)));
    }

    #[tokio::test]
    async fn openai_chat_with_aws_asg_model_auto_wakes() {
        let hits = Arc::new(AtomicUsize::new(0));
        let health_hits = hits.clone();
        let app = Router::new()
            .route(
                "/health",
                axum::routing::get(move || {
                    let health_hits = health_hits.clone();
                    async move {
                        if health_hits.fetch_add(1, Ordering::SeqCst) < 3 {
                            StatusCode::SERVICE_UNAVAILABLE
                        } else {
                            StatusCode::OK
                        }
                    }
                }),
            )
            .route(
                "/v1/chat/completions",
                post(|Json(body): Json<Value>| async move {
                    Json(json!({
                        "id": "chatcmpl-aws",
                        "object": "chat.completion",
                        "model": body["model"],
                        "choices": [{
                            "index": 0,
                            "message": {"role": "assistant", "content": "hello from aws"},
                            "finish_reason": "stop"
                        }]
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let (router, runner) = aws_router(format!("http://{addr}"), RuntimeMode::Fast);
        let resp = router
            .openai_chat(
                None,
                json!({"model":"aws-echo","messages":[{"role":"user","content":"hello"}],"stream":false}),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.body["choices"][0]["message"]["content"],
            "hello from aws"
        );
        assert!(!runner.calls.lock().await.is_empty());
    }
}
