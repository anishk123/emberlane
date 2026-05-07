use crate::{
    config::{EmberlaneConfig, S3StorageConfig},
    error::EmberlaneError,
    model::{FileRecord, StorageBackend},
    provider::{CommandRunner, RealCommandRunner},
    storage::Storage,
    util,
};
use async_trait::async_trait;
use chrono::{Datelike, Utc};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[async_trait]
pub trait ArtifactStore: Send + Sync {
    async fn put_file(
        &self,
        source_path: &Path,
        original_name: &str,
        mime_type: Option<String>,
    ) -> Result<FileRecord, EmberlaneError>;
    async fn get_file_bytes(&self, metadata: &FileRecord) -> Result<Vec<u8>, EmberlaneError>;
    async fn presign_get(
        &self,
        metadata: &FileRecord,
        expires_secs: u64,
    ) -> Result<Option<String>, EmberlaneError>;
    #[allow(dead_code)]
    async fn describe(&self, metadata: &FileRecord) -> Result<Value, EmberlaneError>;
}

#[allow(dead_code)]
pub fn artifact_store(cfg: &EmberlaneConfig) -> Result<Arc<dyn ArtifactStore>, EmberlaneError> {
    artifact_store_with_runner(cfg, Arc::new(RealCommandRunner))
}

pub fn artifact_store_with_runner(
    cfg: &EmberlaneConfig,
    runner: Arc<dyn CommandRunner>,
) -> Result<Arc<dyn ArtifactStore>, EmberlaneError> {
    match cfg.storage.backend {
        StorageBackend::Local => Ok(Arc::new(LocalArtifactStore::new(cfg.files_dir()))),
        StorageBackend::S3 => {
            let s3 = cfg.storage.s3.clone().ok_or_else(|| {
                EmberlaneError::StorageNotConfigured(
                    "storage.backend = \"s3\" requires [storage.s3]".to_string(),
                )
            })?;
            validate_s3_config(&s3)?;
            Ok(Arc::new(S3ArtifactStore::new(s3, runner)))
        }
    }
}

#[allow(dead_code)]
pub async fn store_path(
    cfg: &EmberlaneConfig,
    storage: &Storage,
    path: impl AsRef<Path>,
) -> Result<FileRecord, EmberlaneError> {
    let path = path.as_ref();
    let name = util::safe_file_name(path);
    let mime = mime_guess::from_path(path)
        .first()
        .map(|m| m.essence_str().to_string());
    let record = artifact_store(cfg)?.put_file(path, &name, mime).await?;
    storage.insert_file(&record)?;
    Ok(record)
}

#[allow(dead_code)]
pub async fn store_bytes(
    cfg: &EmberlaneConfig,
    storage: &Storage,
    original_name: &str,
    bytes: &[u8],
) -> Result<FileRecord, EmberlaneError> {
    let tmp_dir = cfg.server.data_dir.join("tmp");
    std::fs::create_dir_all(&tmp_dir)?;
    let tmp_path = tmp_dir.join(format!("upload-{}", util::uuid()));
    std::fs::write(&tmp_path, bytes)?;
    let mime = mime_guess::from_path(original_name)
        .first()
        .map(|m| m.essence_str().to_string());
    let result = artifact_store(cfg)?
        .put_file(&tmp_path, original_name, mime)
        .await;
    let _ = std::fs::remove_file(&tmp_path);
    let record = result?;
    storage.insert_file(&record)?;
    Ok(record)
}

pub async fn storage_status(cfg: &EmberlaneConfig, check: bool) -> Result<Value, EmberlaneError> {
    match cfg.storage.backend {
        StorageBackend::Local => Ok(json!({
            "backend": "local",
            "data_dir": cfg.storage.local.data_dir,
            "files_dir": cfg.files_dir(),
            "check": if check { Some(json!({"ok": cfg.files_dir().exists()})) } else { None }
        })),
        StorageBackend::S3 => {
            let s3 = cfg.storage.s3.as_ref().ok_or_else(|| {
                EmberlaneError::StorageNotConfigured(
                    "storage.backend = \"s3\" requires [storage.s3]".to_string(),
                )
            })?;
            validate_s3_config(s3)?;
            let mut value = json!({
                "backend": "s3",
                "bucket": s3.bucket,
                "prefix": normalize_prefix(&s3.prefix),
                "region": s3.region,
                "presign_downloads": s3.presign_downloads,
                "presign_expires_secs": s3.presign_expires_secs,
                "pass_s3_uri": s3.pass_s3_uri
            });
            if check {
                let runner = RealCommandRunner;
                let args = s3_ls_args(s3);
                let out = runner.run(&s3.aws_cli, &args).await?;
                value["check"] = json!({
                    "ok": out.status == 0,
                    "message": if out.status == 0 { out.stdout } else { out.stderr }
                });
            }
            Ok(value)
        }
    }
}

pub struct LocalArtifactStore {
    files_dir: PathBuf,
}

impl LocalArtifactStore {
    pub fn new(files_dir: PathBuf) -> Self {
        Self { files_dir }
    }
}

#[async_trait]
impl ArtifactStore for LocalArtifactStore {
    async fn put_file(
        &self,
        source_path: &Path,
        original_name: &str,
        mime_type: Option<String>,
    ) -> Result<FileRecord, EmberlaneError> {
        validate_local_original_name(original_name)?;
        let bytes = std::fs::read(source_path)?;
        std::fs::create_dir_all(&self.files_dir)?;
        let id = util::uuid();
        let safe = sanitize_file_name(original_name)?;
        let stored = self.files_dir.join(format!("{id}-{safe}"));
        std::fs::write(&stored, &bytes)?;
        Ok(FileRecord {
            id,
            original_name: original_name.to_string(),
            stored_path: Some(stored.to_string_lossy().to_string()),
            storage_backend: StorageBackend::Local,
            storage_key: None,
            bucket: None,
            region: None,
            s3_uri: None,
            mime_type,
            size_bytes: bytes.len() as i64,
            sha256: Some(util::sha256_hex(&bytes)),
            created_at: util::now(),
        })
    }

    async fn get_file_bytes(&self, metadata: &FileRecord) -> Result<Vec<u8>, EmberlaneError> {
        if metadata.storage_backend != StorageBackend::Local {
            return Err(EmberlaneError::StorageBackendUnsupported(
                "local store cannot read non-local metadata".to_string(),
            ));
        }
        let path = metadata.stored_path.as_ref().ok_or_else(|| {
            EmberlaneError::StorageNotConfigured(
                "local file metadata missing stored_path".to_string(),
            )
        })?;
        std::fs::read(path).map_err(Into::into)
    }

    async fn presign_get(
        &self,
        _metadata: &FileRecord,
        _expires_secs: u64,
    ) -> Result<Option<String>, EmberlaneError> {
        Err(EmberlaneError::PresignNotSupported)
    }

    async fn describe(&self, metadata: &FileRecord) -> Result<Value, EmberlaneError> {
        Ok(file_reference(metadata, None, true))
    }
}

pub struct S3ArtifactStore {
    cfg: S3StorageConfig,
    runner: Arc<dyn CommandRunner>,
}

impl S3ArtifactStore {
    pub fn new(cfg: S3StorageConfig, runner: Arc<dyn CommandRunner>) -> Self {
        Self { cfg, runner }
    }
}

#[async_trait]
impl ArtifactStore for S3ArtifactStore {
    async fn put_file(
        &self,
        source_path: &Path,
        original_name: &str,
        mime_type: Option<String>,
    ) -> Result<FileRecord, EmberlaneError> {
        validate_s3_config(&self.cfg)?;
        let bytes = std::fs::read(source_path)?;
        let id = util::uuid();
        let safe = sanitize_file_name(original_name)?;
        let key = s3_key(&self.cfg.prefix, &id, &safe)?;
        let s3_uri = format!("s3://{}/{}", self.cfg.bucket, key);
        let args = s3_cp_upload_args(&self.cfg, source_path, &s3_uri);
        let out = self.runner.run(&self.cfg.aws_cli, &args).await?;
        if out.status != 0 {
            return Err(EmberlaneError::S3UploadFailed(clean_stderr(&out.stderr)));
        }
        Ok(FileRecord {
            id,
            original_name: original_name.to_string(),
            stored_path: None,
            storage_backend: StorageBackend::S3,
            storage_key: Some(key),
            bucket: Some(self.cfg.bucket.clone()),
            region: Some(self.cfg.region.clone()),
            s3_uri: Some(s3_uri),
            mime_type,
            size_bytes: bytes.len() as i64,
            sha256: Some(util::sha256_hex(&bytes)),
            created_at: util::now(),
        })
    }

    async fn get_file_bytes(&self, metadata: &FileRecord) -> Result<Vec<u8>, EmberlaneError> {
        let s3_uri = metadata.s3_uri.as_ref().ok_or_else(|| {
            EmberlaneError::StorageNotConfigured("S3 file metadata missing s3_uri".to_string())
        })?;
        let args = s3_cp_download_args(&self.cfg, s3_uri);
        let out = self.runner.run(&self.cfg.aws_cli, &args).await?;
        if out.status != 0 {
            return Err(EmberlaneError::S3DownloadFailed(clean_stderr(&out.stderr)));
        }
        Ok(out.stdout.into_bytes())
    }

    async fn presign_get(
        &self,
        metadata: &FileRecord,
        expires_secs: u64,
    ) -> Result<Option<String>, EmberlaneError> {
        let s3_uri = metadata.s3_uri.as_ref().ok_or_else(|| {
            EmberlaneError::StorageNotConfigured("S3 file metadata missing s3_uri".to_string())
        })?;
        let args = s3_presign_args(&self.cfg, s3_uri, expires_secs);
        let out = self.runner.run(&self.cfg.aws_cli, &args).await?;
        if out.status != 0 {
            return Err(EmberlaneError::S3PresignFailed(clean_stderr(&out.stderr)));
        }
        Ok(Some(out.stdout.trim().to_string()))
    }

    async fn describe(&self, metadata: &FileRecord) -> Result<Value, EmberlaneError> {
        let presigned_url = if self.cfg.presign_downloads {
            self.presign_get(metadata, self.cfg.presign_expires_secs)
                .await?
        } else {
            None
        };
        Ok(file_reference(metadata, presigned_url, false))
    }
}

pub fn file_reference(
    metadata: &FileRecord,
    presigned_url: Option<String>,
    include_local_path: bool,
) -> Value {
    let mut value = json!({
        "file_id": metadata.id,
        "original_name": metadata.original_name,
        "storage_backend": metadata.storage_backend,
        "storage_key": metadata.storage_key,
        "bucket": metadata.bucket,
        "region": metadata.region,
        "s3_uri": metadata.s3_uri,
        "mime_type": metadata.mime_type,
        "size_bytes": metadata.size_bytes,
        "sha256": metadata.sha256
    });
    if include_local_path {
        value["stored_path"] = json!(metadata.stored_path);
    }
    if let Some(url) = presigned_url {
        value["presigned_url"] = json!(url);
    }
    value
}

fn validate_s3_config(cfg: &S3StorageConfig) -> Result<(), EmberlaneError> {
    if cfg.bucket.trim().is_empty() {
        return Err(EmberlaneError::StorageNotConfigured(
            "storage.s3.bucket is required".to_string(),
        ));
    }
    if cfg.region.trim().is_empty() {
        return Err(EmberlaneError::StorageNotConfigured(
            "storage.s3.region is required".to_string(),
        ));
    }
    Ok(())
}

fn validate_local_original_name(original_name: &str) -> Result<(), EmberlaneError> {
    if original_name.contains("..") || original_name.contains('/') || original_name.contains('\\') {
        return Err(EmberlaneError::UnsafeFileName(original_name.to_string()));
    }
    sanitize_file_name(original_name).map(|_| ())
}

pub fn sanitize_file_name(original_name: &str) -> Result<String, EmberlaneError> {
    let leaf = Path::new(original_name)
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or(original_name);
    let safe = leaf
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('.')
        .to_string();
    if safe.is_empty() || safe == "." || safe == ".." {
        Err(EmberlaneError::UnsafeFileName(original_name.to_string()))
    } else {
        Ok(safe)
    }
}

pub fn s3_key(prefix: &str, file_id: &str, original_name: &str) -> Result<String, EmberlaneError> {
    let safe = sanitize_file_name(original_name)?;
    let now = Utc::now();
    let prefix = normalize_prefix(prefix);
    let key = format!(
        "{}{:04}/{:02}/{:02}/{}/{}",
        prefix,
        now.year(),
        now.month(),
        now.day(),
        file_id,
        safe
    );
    if key.split('/').any(|part| part == "..") {
        return Err(EmberlaneError::UnsafeFileName(original_name.to_string()));
    }
    Ok(key)
}

fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}

fn s3_cp_upload_args(cfg: &S3StorageConfig, source_path: &Path, s3_uri: &str) -> Vec<String> {
    let mut args = vec![
        "s3".to_string(),
        "cp".to_string(),
        source_path.to_string_lossy().to_string(),
        s3_uri.to_string(),
        "--region".to_string(),
        cfg.region.clone(),
    ];
    add_profile_args(&mut args, cfg);
    args
}

fn s3_cp_download_args(cfg: &S3StorageConfig, s3_uri: &str) -> Vec<String> {
    let mut args = vec![
        "s3".to_string(),
        "cp".to_string(),
        s3_uri.to_string(),
        "-".to_string(),
        "--region".to_string(),
        cfg.region.clone(),
    ];
    add_profile_args(&mut args, cfg);
    args
}

fn s3_presign_args(cfg: &S3StorageConfig, s3_uri: &str, expires_secs: u64) -> Vec<String> {
    let mut args = vec![
        "s3".to_string(),
        "presign".to_string(),
        s3_uri.to_string(),
        "--expires-in".to_string(),
        expires_secs.to_string(),
        "--region".to_string(),
        cfg.region.clone(),
    ];
    add_profile_args(&mut args, cfg);
    args
}

fn s3_ls_args(cfg: &S3StorageConfig) -> Vec<String> {
    let prefix = normalize_prefix(&cfg.prefix);
    let mut args = vec![
        "s3".to_string(),
        "ls".to_string(),
        format!("s3://{}/{}", cfg.bucket, prefix),
        "--region".to_string(),
        cfg.region.clone(),
    ];
    add_profile_args(&mut args, cfg);
    args
}

fn add_profile_args(args: &mut Vec<String>, cfg: &S3StorageConfig) {
    if let Some(profile) = cfg.profile.as_deref().filter(|v| !v.trim().is_empty()) {
        args.push("--profile".to_string());
        args.push(profile.to_string());
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

#[allow(dead_code)]
pub fn ensure_inside_files_dir(
    cfg: &EmberlaneConfig,
    path: &Path,
) -> Result<PathBuf, EmberlaneError> {
    let files_dir = cfg.files_dir().canonicalize()?;
    let path = path.canonicalize()?;
    if !path.starts_with(files_dir) {
        return Err(EmberlaneError::InvalidRequest(
            "path traversal rejected".to_string(),
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{CommandOutput, CommandRunner};
    use std::{collections::VecDeque, sync::Mutex};

    #[derive(Default)]
    struct FakeCommandRunner {
        calls: Mutex<Vec<(String, Vec<String>)>>,
        outputs: Mutex<VecDeque<CommandOutput>>,
    }

    impl FakeCommandRunner {
        fn push(&self, output: CommandOutput) {
            self.outputs.lock().unwrap().push_back(output);
        }
        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
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
                .unwrap()
                .push((program.to_string(), args.to_vec()));
            Ok(self
                .outputs
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }))
        }
    }

    fn s3_cfg() -> S3StorageConfig {
        S3StorageConfig {
            bucket: "bucket".to_string(),
            prefix: "uploads/".to_string(),
            region: "us-west-2".to_string(),
            aws_cli: "aws".to_string(),
            profile: Some("dev".to_string()),
            presign_downloads: true,
            presign_expires_secs: 900,
            pass_s3_uri: true,
        }
    }

    #[tokio::test]
    async fn local_upload_still_works_and_records_backend() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = EmberlaneConfig::default();
        cfg.storage.local.data_dir = dir.path().join(".emberlane");
        let db = Storage::open_memory().unwrap();
        let file = store_bytes(&cfg, &db, "a.txt", b"hello").await.unwrap();
        assert_eq!(file.storage_backend, StorageBackend::Local);
        assert!(file.stored_path.unwrap().contains("a.txt"));
        assert_eq!(file.sha256.unwrap().len(), 64);
        assert!(store_bytes(&cfg, &db, "../bad.txt", b"x").await.is_err());
    }

    #[tokio::test]
    async fn s3_upload_calls_aws_and_records_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("note.md");
        std::fs::write(&source, "hello s3").unwrap();
        let runner = Arc::new(FakeCommandRunner::default());
        let store = S3ArtifactStore::new(s3_cfg(), runner.clone());
        let file = store
            .put_file(&source, "../note.md", Some("text/markdown".to_string()))
            .await
            .unwrap();
        assert_eq!(file.storage_backend, StorageBackend::S3);
        assert_eq!(file.bucket.as_deref(), Some("bucket"));
        assert_eq!(file.region.as_deref(), Some("us-west-2"));
        assert!(file
            .s3_uri
            .as_ref()
            .unwrap()
            .starts_with("s3://bucket/uploads/"));
        assert!(!file.storage_key.as_ref().unwrap().contains(".."));
        let calls = runner.calls();
        assert_eq!(calls[0].0, "aws");
        assert!(calls[0].1.contains(&"cp".to_string()));
        assert!(calls[0].1.contains(&"--profile".to_string()));
    }

    #[tokio::test]
    async fn s3_presign_and_download_use_expected_commands() {
        let runner = Arc::new(FakeCommandRunner::default());
        runner.push(CommandOutput {
            status: 0,
            stdout: "hello from s3".to_string(),
            stderr: String::new(),
        });
        runner.push(CommandOutput {
            status: 0,
            stdout: "https://example.test/presigned\n".to_string(),
            stderr: String::new(),
        });
        let store = S3ArtifactStore::new(s3_cfg(), runner.clone());
        let file = FileRecord {
            id: "f".to_string(),
            original_name: "note.md".to_string(),
            stored_path: None,
            storage_backend: StorageBackend::S3,
            storage_key: Some("uploads/2026/05/04/f/note.md".to_string()),
            bucket: Some("bucket".to_string()),
            region: Some("us-west-2".to_string()),
            s3_uri: Some("s3://bucket/uploads/2026/05/04/f/note.md".to_string()),
            mime_type: Some("text/markdown".to_string()),
            size_bytes: 5,
            sha256: Some("x".to_string()),
            created_at: util::now(),
        };
        assert_eq!(store.get_file_bytes(&file).await.unwrap(), b"hello from s3");
        let url = store.presign_get(&file, 60).await.unwrap().unwrap();
        assert_eq!(url, "https://example.test/presigned");
        let calls = runner.calls();
        assert!(calls[0].1.contains(&"-".to_string()));
        assert!(calls[1].1.contains(&"presign".to_string()));
        assert!(calls[1].1.contains(&"60".to_string()));
    }

    #[tokio::test]
    async fn presign_local_file_returns_not_supported() {
        let store = LocalArtifactStore::new(PathBuf::from(".emberlane/files"));
        let file = FileRecord {
            id: "f".to_string(),
            original_name: "a.md".to_string(),
            stored_path: Some("/tmp/a.md".to_string()),
            storage_backend: StorageBackend::Local,
            storage_key: None,
            bucket: None,
            region: None,
            s3_uri: None,
            mime_type: None,
            size_bytes: 1,
            sha256: None,
            created_at: util::now(),
        };
        let err = store.presign_get(&file, 60).await.unwrap_err();
        assert!(matches!(err, EmberlaneError::PresignNotSupported));
    }

    #[test]
    fn s3_key_generation_is_safe() {
        let key = s3_key("/uploads//", "file-id", "../bad name.md").unwrap();
        assert!(key.starts_with("uploads/"));
        assert!(key.contains("/file-id/bad_name.md"));
        assert!(!key.contains(".."));
        assert!(!key.contains("//"));
    }
}
