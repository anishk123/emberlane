use crate::{
    error::EmberlaneError,
    model::{ProviderKind, RuntimeConfig, RuntimeMode, StorageBackend},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{env, fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub api_key: Option<String>,
    pub data_dir: PathBuf,
    pub default_runtime_id: String,
    pub default_ollama_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmberlaneConfig {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub runtimes: Vec<RuntimeConfig>,
    #[serde(skip)]
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default)]
    pub backend: StorageBackend,
    #[serde(default = "default_inline_file_max_bytes")]
    pub inline_file_max_bytes: u64,
    #[serde(default)]
    pub local: LocalStorageConfig,
    pub s3: Option<S3StorageConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalStorageConfig {
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3StorageConfig {
    pub bucket: String,
    #[serde(default = "default_s3_prefix")]
    pub prefix: String,
    pub region: String,
    #[serde(default = "default_aws_cli")]
    pub aws_cli: String,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default = "default_true")]
    pub presign_downloads: bool,
    #[serde(default = "default_presign_expires")]
    pub presign_expires_secs: u64,
    #[serde(default = "default_true")]
    pub pass_s3_uri: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8787,
            api_key: Some("dev-secret".to_string()),
            data_dir: PathBuf::from(".emberlane"),
            default_runtime_id: "echo".to_string(),
            default_ollama_model: "llama3.2:1b".to_string(),
        }
    }
}

impl Default for EmberlaneConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            storage: StorageConfig::default(),
            runtimes: vec![default_echo_runtime(), default_ollama_runtime()],
            config_path: None,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            backend: StorageBackend::Local,
            inline_file_max_bytes: default_inline_file_max_bytes(),
            local: LocalStorageConfig::default(),
            s3: None,
        }
    }
}

impl Default for LocalStorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(".emberlane"),
        }
    }
}

impl EmberlaneConfig {
    pub fn discover(config: Option<PathBuf>) -> Result<Self, EmberlaneError> {
        let path = if let Some(path) = config {
            Some(path)
        } else if let Ok(path) = env::var("EMBERLANE_CONFIG") {
            Some(PathBuf::from(path))
        } else if PathBuf::from("emberlane.toml").exists() {
            Some(PathBuf::from("emberlane.toml"))
        } else {
            dirs::config_dir()
                .map(|p| p.join("emberlane").join("config.toml"))
                .filter(|p| p.exists())
        };

        if let Some(path) = path {
            let text = fs::read_to_string(&path).map_err(|e| {
                EmberlaneError::InvalidRequest(format!("failed to read {}: {e}", path.display()))
            })?;
            let mut cfg: EmberlaneConfig = toml::from_str(&text).map_err(|e| {
                EmberlaneError::InvalidRequest(format!("failed to parse {}: {e}", path.display()))
            })?;
            cfg.config_path = Some(path);
            Ok(cfg)
        } else {
            Ok(Self::default())
        }
    }

    pub fn api_key(&self) -> Option<String> {
        self.server.api_key.clone()
    }

    pub fn db_path(&self) -> PathBuf {
        self.server.data_dir.join("emberlane.db")
    }

    pub fn files_dir(&self) -> PathBuf {
        self.storage.local.data_dir.join("files")
    }

    pub fn write_default(path: PathBuf, force: bool) -> Result<(), EmberlaneError> {
        if path.exists() && !force {
            return Err(EmberlaneError::InvalidRequest(format!(
                "{} already exists; use --force to overwrite",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, DEFAULT_CONFIG_TOML)?;
        Ok(())
    }

    pub fn write_to(&self, path: PathBuf, force: bool) -> Result<(), EmberlaneError> {
        if path.exists() && !force {
            return Err(EmberlaneError::InvalidRequest(format!(
                "{} already exists; use --force to overwrite",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| {
            EmberlaneError::InvalidRequest(format!("failed to render {}: {e}", path.display()))
        })?;
        fs::write(path, text)?;
        Ok(())
    }
}

const DEFAULT_CONFIG_TOML: &str = r#"[server]
host = "127.0.0.1"
port = 8787
api_key = "dev-secret"
data_dir = ".emberlane"
default_runtime_id = "echo"
default_ollama_model = "llama3.2:1b"

[storage]
backend = "local"
inline_file_max_bytes = 200000

[storage.local]
data_dir = ".emberlane"

[[runtimes]]
id = "echo"
name = "Echo Runtime"
provider = "local_process"
enabled = true
mode = "fast"
base_url = "http://127.0.0.1:9001"
health_path = "/health"
startup_timeout_secs = 20
fast_wait_secs = 10
slow_retry_after_secs = 2
idle_ttl_secs = 300
max_concurrency = 4

[runtimes.config]
command = "python"
args = ["examples/echo-runtime/server.py"]
cwd = "."
env = { PORT = "9001" }

[[runtimes]]
id = "ollama"
name = "Local Ollama"
provider = "ollama"
enabled = true
mode = "fast"
base_url = "http://127.0.0.1:11434"
health_path = "/api/tags"
startup_timeout_secs = 20
fast_wait_secs = 8
slow_retry_after_secs = 2
idle_ttl_secs = 300
max_concurrency = 2

[runtimes.config]
command = "ollama"
args = ["serve"]
model = "llama3.2:1b"
"#;

fn default_inline_file_max_bytes() -> u64 {
    200_000
}

fn default_s3_prefix() -> String {
    "uploads/".to_string()
}

fn default_aws_cli() -> String {
    "aws".to_string()
}

fn default_presign_expires() -> u64 {
    900
}

fn default_true() -> bool {
    true
}

pub fn default_echo_runtime() -> RuntimeConfig {
    RuntimeConfig {
        id: "echo".to_string(),
        name: "Echo Runtime".to_string(),
        provider: ProviderKind::LocalProcess,
        enabled: true,
        mode: RuntimeMode::Fast,
        base_url: Some("http://127.0.0.1:9001".to_string()),
        health_path: "/health".to_string(),
        startup_timeout_secs: 20,
        fast_wait_secs: 10,
        slow_retry_after_secs: 2,
        idle_ttl_secs: Some(300),
        max_concurrency: Some(4),
        config: json!({
            "command": "python",
            "args": ["examples/echo-runtime/server.py"],
            "cwd": ".",
            "env": {"PORT": "9001"}
        }),
    }
}

pub fn default_ollama_runtime() -> RuntimeConfig {
    RuntimeConfig {
        id: "ollama".to_string(),
        name: "Local Ollama".to_string(),
        provider: ProviderKind::Ollama,
        enabled: true,
        mode: RuntimeMode::Fast,
        base_url: Some("http://127.0.0.1:11434".to_string()),
        health_path: "/api/tags".to_string(),
        startup_timeout_secs: 20,
        fast_wait_secs: 8,
        slow_retry_after_secs: 2,
        idle_ttl_secs: Some(300),
        max_concurrency: Some(2),
        config: json!({
            "command": "ollama",
            "args": ["serve"],
            "model": "llama3.2:1b"
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Storage;

    #[test]
    fn discovery_uses_local_emberlane_toml() {
        let cwd = env::current_dir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        env::set_current_dir(dir.path()).unwrap();
        EmberlaneConfig::write_default(PathBuf::from("emberlane.toml"), false).unwrap();
        let cfg = EmberlaneConfig::discover(None).unwrap();
        env::set_current_dir(cwd).unwrap();
        assert_eq!(cfg.server.default_runtime_id, "echo");
        assert_eq!(cfg.storage.backend, StorageBackend::Local);
        assert_eq!(cfg.config_path.unwrap(), PathBuf::from("emberlane.toml"));
    }

    #[test]
    fn init_creates_config_and_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emberlane.toml");
        EmberlaneConfig::write_default(path.clone(), false).unwrap();
        let mut cfg = EmberlaneConfig::discover(Some(path)).unwrap();
        cfg.server.data_dir = dir.path().join(".emberlane");
        cfg.storage.local.data_dir = dir.path().join(".emberlane");
        fs::create_dir_all(cfg.files_dir()).unwrap();
        let storage = Storage::open(cfg.db_path()).unwrap();
        for runtime in &cfg.runtimes {
            storage.upsert_runtime(runtime).unwrap();
        }
        assert!(cfg.server.data_dir.exists());
        assert!(cfg.files_dir().exists());
        assert_eq!(storage.list_runtimes().unwrap().len(), 2);
    }

    #[test]
    fn init_does_not_overwrite_without_force() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emberlane.toml");
        EmberlaneConfig::write_default(path.clone(), false).unwrap();
        let err = EmberlaneConfig::write_default(path, false).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn generated_config_uses_local_storage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("emberlane.toml");
        EmberlaneConfig::write_default(path.clone(), false).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("[storage]"));
        assert!(text.contains("backend = \"local\""));
        let cfg = EmberlaneConfig::discover(Some(path)).unwrap();
        assert_eq!(cfg.storage.backend, StorageBackend::Local);
        assert_eq!(cfg.storage.inline_file_max_bytes, 200_000);
    }

    #[test]
    fn partial_storage_config_uses_default_server() {
        let cfg: EmberlaneConfig = toml::from_str(
            r#"
            [storage]
            backend = "s3"

            [storage.s3]
            bucket = "bucket"
            region = "us-west-2"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.server.default_runtime_id, "echo");
        assert_eq!(cfg.storage.backend, StorageBackend::S3);
        assert_eq!(cfg.storage.s3.unwrap().bucket, "bucket");
    }

    #[test]
    fn s3_config_parses() {
        let cfg: EmberlaneConfig = toml::from_str(
            r#"
            [server]
            host = "127.0.0.1"
            port = 8787
            data_dir = ".emberlane"
            default_runtime_id = "echo"
            default_ollama_model = "llama3.2:1b"

            [storage]
            backend = "s3"
            inline_file_max_bytes = 123

            [storage.local]
            data_dir = ".emberlane"

            [storage.s3]
            bucket = "bucket"
            prefix = "uploads/"
            region = "us-west-2"
            profile = "dev"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.storage.backend, StorageBackend::S3);
        let s3 = cfg.storage.s3.unwrap();
        assert_eq!(s3.bucket, "bucket");
        assert_eq!(s3.aws_cli, "aws");
        assert!(s3.presign_downloads);
        assert_eq!(s3.presign_expires_secs, 900);
        assert!(s3.pass_s3_uri);
    }
}
