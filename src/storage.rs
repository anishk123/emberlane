use crate::{
    error::EmberlaneError,
    model::{
        EventRecord, FileRecord, ProviderKind, RuntimeConfig, RuntimeMode, RuntimeState,
        RuntimeStateKind, RuntimeStatus, StorageBackend,
    },
    util,
};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::{
    path::Path,
    str::FromStr,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, EmberlaneError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    #[allow(dead_code)]
    pub fn open_memory() -> Result<Self, EmberlaneError> {
        let storage = Self {
            conn: Arc::new(Mutex::new(Connection::open_in_memory()?)),
        };
        storage.init_schema()?;
        Ok(storage)
    }

    pub fn init_schema(&self) -> Result<(), EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS runtimes (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              provider TEXT NOT NULL,
              enabled INTEGER NOT NULL DEFAULT 1,
              mode TEXT NOT NULL DEFAULT 'fast',
              base_url TEXT,
              health_path TEXT NOT NULL DEFAULT '/health',
              startup_timeout_secs INTEGER NOT NULL DEFAULT 20,
              fast_wait_secs INTEGER NOT NULL DEFAULT 10,
              slow_retry_after_secs INTEGER NOT NULL DEFAULT 2,
              idle_ttl_secs INTEGER,
              max_concurrency INTEGER,
              config_json TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS runtime_state (
              runtime_id TEXT PRIMARY KEY,
              state TEXT NOT NULL,
              last_health_at TEXT,
              last_wake_at TEXT,
              last_ready_at TEXT,
              last_used_at TEXT,
              last_error TEXT,
              in_flight INTEGER DEFAULT 0
            );
            CREATE TABLE IF NOT EXISTS files (
              id TEXT PRIMARY KEY,
              original_name TEXT NOT NULL,
              stored_path TEXT,
              storage_backend TEXT NOT NULL DEFAULT 'local',
              storage_key TEXT,
              bucket TEXT,
              region TEXT,
              s3_uri TEXT,
              mime_type TEXT,
              size_bytes INTEGER NOT NULL,
              sha256 TEXT,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
              id TEXT PRIMARY KEY,
              runtime_id TEXT,
              event_type TEXT NOT NULL,
              message TEXT,
              data_json TEXT,
              created_at TEXT NOT NULL
            );
            "#,
        )?;
        migrate_files_schema(&conn)?;
        Ok(())
    }

    pub fn upsert_runtime(&self, runtime: &RuntimeConfig) -> Result<(), EmberlaneError> {
        let now = util::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO runtimes (
              id, name, provider, enabled, mode, base_url, health_path,
              startup_timeout_secs, fast_wait_secs, slow_retry_after_secs,
              idle_ttl_secs, max_concurrency, config_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
            ON CONFLICT(id) DO UPDATE SET
              name=excluded.name,
              provider=excluded.provider,
              enabled=excluded.enabled,
              mode=excluded.mode,
              base_url=excluded.base_url,
              health_path=excluded.health_path,
              startup_timeout_secs=excluded.startup_timeout_secs,
              fast_wait_secs=excluded.fast_wait_secs,
              slow_retry_after_secs=excluded.slow_retry_after_secs,
              idle_ttl_secs=excluded.idle_ttl_secs,
              max_concurrency=excluded.max_concurrency,
              config_json=excluded.config_json,
              updated_at=excluded.updated_at
            "#,
            params![
                runtime.id,
                runtime.name,
                runtime.provider.to_string(),
                runtime.enabled as i32,
                runtime.mode.to_string(),
                runtime.base_url,
                runtime.health_path,
                runtime.startup_timeout_secs as i64,
                runtime.fast_wait_secs as i64,
                runtime.slow_retry_after_secs as i64,
                runtime.idle_ttl_secs.map(|v| v as i64),
                runtime.max_concurrency.map(|v| v as i64),
                serde_json::to_string(&runtime.config).unwrap(),
                now,
            ],
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO runtime_state (runtime_id, state, in_flight) VALUES (?1, 'unknown', 0)",
            params![runtime.id],
        )?;
        drop(conn);
        self.record_event(
            Some(&runtime.id),
            "runtime_registered",
            Some("runtime registered"),
            Some(json!({"provider": runtime.provider.to_string()})),
        )?;
        Ok(())
    }

    pub fn list_runtimes(&self) -> Result<Vec<RuntimeConfig>, EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(runtime_select_sql("ORDER BY id").as_str())?;
        let rows = stmt.query_map([], runtime_from_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn load_runtime(&self, id: &str) -> Result<Option<RuntimeConfig>, EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            runtime_select_sql("WHERE id=?1").as_str(),
            params![id],
            runtime_from_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn set_runtime_state(
        &self,
        runtime_id: &str,
        state: RuntimeStateKind,
        error: Option<String>,
    ) -> Result<(), EmberlaneError> {
        let now = util::now().to_rfc3339();
        let (health, wake, ready, used) = match state {
            RuntimeStateKind::Ready => (
                Some(now.clone()),
                None,
                Some(now.clone()),
                Some(now.clone()),
            ),
            RuntimeStateKind::Waking => (None, Some(now.clone()), None, None),
            _ => (None, None, None, None),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO runtime_state (runtime_id, state, last_health_at, last_wake_at, last_ready_at, last_used_at, last_error, in_flight)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)
            ON CONFLICT(runtime_id) DO UPDATE SET
              state=excluded.state,
              last_health_at=COALESCE(excluded.last_health_at, runtime_state.last_health_at),
              last_wake_at=COALESCE(excluded.last_wake_at, runtime_state.last_wake_at),
              last_ready_at=COALESCE(excluded.last_ready_at, runtime_state.last_ready_at),
              last_used_at=COALESCE(excluded.last_used_at, runtime_state.last_used_at),
              last_error=excluded.last_error
            "#,
            params![runtime_id, state.to_string(), health, wake, ready, used, error],
        )?;
        Ok(())
    }

    pub fn get_runtime_state(&self, runtime_id: &str) -> Result<RuntimeState, EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT runtime_id, state, last_health_at, last_wake_at, last_ready_at, last_used_at, last_error, in_flight FROM runtime_state WHERE runtime_id=?1",
            params![runtime_id],
            state_from_row,
        )
        .optional()?
        .ok_or_else(|| EmberlaneError::RuntimeNotFound(runtime_id.to_string()))
    }

    pub fn list_runtime_status(&self) -> Result<Vec<RuntimeStatus>, EmberlaneError> {
        self.list_runtimes()?
            .into_iter()
            .map(|runtime| {
                let state = self.get_runtime_state(&runtime.id)?;
                Ok(RuntimeStatus {
                    runtime,
                    state,
                    provider_status: None,
                })
            })
            .collect()
    }

    pub fn increment_in_flight(&self, runtime_id: &str) -> Result<(), EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runtime_state SET in_flight=COALESCE(in_flight, 0)+1 WHERE runtime_id=?1",
            params![runtime_id],
        )?;
        Ok(())
    }

    pub fn decrement_in_flight(&self, runtime_id: &str) -> Result<(), EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE runtime_state SET in_flight=MAX(COALESCE(in_flight, 0)-1, 0), last_used_at=?2 WHERE runtime_id=?1",
            params![runtime_id, util::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn insert_file(&self, file: &FileRecord) -> Result<(), EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            r#"
            INSERT INTO files (
              id, original_name, stored_path, storage_backend, storage_key, bucket,
              region, s3_uri, mime_type, size_bytes, sha256, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                &file.id,
                &file.original_name,
                file.stored_path.as_ref(),
                file.storage_backend.to_string(),
                file.storage_key.as_ref(),
                file.bucket.as_ref(),
                file.region.as_ref(),
                file.s3_uri.as_ref(),
                file.mime_type.as_ref(),
                file.size_bytes,
                file.sha256.as_ref(),
                file.created_at.to_rfc3339()
            ],
        )?;
        drop(conn);
        self.record_event(
            None,
            "file_uploaded",
            Some("file uploaded"),
            Some(json!({"file_id": file.id})),
        )?;
        Ok(())
    }

    pub fn get_file(&self, id: &str) -> Result<FileRecord, EmberlaneError> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, original_name, stored_path, storage_backend, storage_key, bucket, region, s3_uri, mime_type, size_bytes, sha256, created_at FROM files WHERE id=?1",
            params![id],
            file_from_row,
        )
        .optional()?
        .ok_or_else(|| EmberlaneError::FileNotFound(id.to_string()))
    }

    pub fn record_event(
        &self,
        runtime_id: Option<&str>,
        event_type: &str,
        message: Option<&str>,
        data_json: Option<Value>,
    ) -> Result<EventRecord, EmberlaneError> {
        let event = EventRecord {
            id: util::uuid(),
            runtime_id: runtime_id.map(ToOwned::to_owned),
            event_type: event_type.to_string(),
            message: message.map(ToOwned::to_owned),
            data_json,
            created_at: util::now(),
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO events (id, runtime_id, event_type, message, data_json, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.id,
                event.runtime_id,
                event.event_type,
                event.message,
                event.data_json.as_ref().map(|v| v.to_string()),
                event.created_at.to_rfc3339()
            ],
        )?;
        Ok(event)
    }
}

fn runtime_select_sql(tail: &str) -> String {
    format!(
        "SELECT id, name, provider, enabled, mode, base_url, health_path, startup_timeout_secs, fast_wait_secs, slow_retry_after_secs, idle_ttl_secs, max_concurrency, config_json FROM runtimes {tail}"
    )
}

fn migrate_files_schema(conn: &Connection) -> Result<(), EmberlaneError> {
    let mut stmt = conn.prepare("PRAGMA table_info(files)")?;
    let columns = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let has_storage_backend = columns.iter().any(|(name, _)| name == "storage_backend");
    let stored_path_not_null = columns
        .iter()
        .find(|(name, _)| name == "stored_path")
        .map(|(_, not_null)| *not_null == 1)
        .unwrap_or(false);
    if has_storage_backend && !stored_path_not_null {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        ALTER TABLE files RENAME TO files_old;
        CREATE TABLE files (
          id TEXT PRIMARY KEY,
          original_name TEXT NOT NULL,
          stored_path TEXT,
          storage_backend TEXT NOT NULL DEFAULT 'local',
          storage_key TEXT,
          bucket TEXT,
          region TEXT,
          s3_uri TEXT,
          mime_type TEXT,
          size_bytes INTEGER NOT NULL,
          sha256 TEXT,
          created_at TEXT NOT NULL
        );
        "#,
    )?;
    let old_columns = columns
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>();
    let has = |name: &str| old_columns.contains(&name);
    let storage_backend_expr = if has("storage_backend") {
        "COALESCE(storage_backend, 'local')"
    } else {
        "'local'"
    };
    let storage_key_expr = if has("storage_key") {
        "storage_key"
    } else {
        "NULL"
    };
    let bucket_expr = if has("bucket") { "bucket" } else { "NULL" };
    let region_expr = if has("region") { "region" } else { "NULL" };
    let s3_uri_expr = if has("s3_uri") { "s3_uri" } else { "NULL" };
    conn.execute_batch(&format!(
        r#"
        INSERT INTO files (
          id, original_name, stored_path, storage_backend, storage_key, bucket,
          region, s3_uri, mime_type, size_bytes, sha256, created_at
        )
        SELECT
          id, original_name, stored_path, {storage_backend_expr}, {storage_key_expr},
          {bucket_expr}, {region_expr}, {s3_uri_expr}, mime_type, size_bytes, sha256, created_at
        FROM files_old;
        DROP TABLE files_old;
        "#
    ))?;
    Ok(())
}

fn runtime_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeConfig> {
    let provider: String = row.get("provider")?;
    let mode: String = row.get("mode")?;
    let config: String = row.get("config_json")?;
    Ok(RuntimeConfig {
        id: row.get("id")?,
        name: row.get("name")?,
        provider: ProviderKind::from_str(&provider).map_err(to_sql_string_error)?,
        enabled: row.get::<_, i64>("enabled")? != 0,
        mode: RuntimeMode::from_str(&mode).map_err(to_sql_string_error)?,
        base_url: row.get("base_url")?,
        health_path: row.get("health_path")?,
        startup_timeout_secs: row.get::<_, i64>("startup_timeout_secs")? as u64,
        fast_wait_secs: row.get::<_, i64>("fast_wait_secs")? as u64,
        slow_retry_after_secs: row.get::<_, i64>("slow_retry_after_secs")? as u64,
        idle_ttl_secs: row
            .get::<_, Option<i64>>("idle_ttl_secs")?
            .map(|v| v as u64),
        max_concurrency: row
            .get::<_, Option<i64>>("max_concurrency")?
            .map(|v| v as u32),
        config: serde_json::from_str(&config).map_err(to_sql_error)?,
    })
}

fn state_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RuntimeState> {
    let state: String = row.get("state")?;
    Ok(RuntimeState {
        runtime_id: row.get("runtime_id")?,
        state: RuntimeStateKind::from_str(&state).map_err(to_sql_string_error)?,
        last_health_at: parse_time(row.get("last_health_at")?)?,
        last_wake_at: parse_time(row.get("last_wake_at")?)?,
        last_ready_at: parse_time(row.get("last_ready_at")?)?,
        last_used_at: parse_time(row.get("last_used_at")?)?,
        last_error: row.get("last_error")?,
        in_flight: row.get("in_flight")?,
    })
}

fn file_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FileRecord> {
    let backend: String = row.get("storage_backend")?;
    Ok(FileRecord {
        id: row.get("id")?,
        original_name: row.get("original_name")?,
        stored_path: row.get("stored_path")?,
        storage_backend: StorageBackend::from_str(&backend).map_err(to_sql_string_error)?,
        storage_key: row.get("storage_key")?,
        bucket: row.get("bucket")?,
        region: row.get("region")?,
        s3_uri: row.get("s3_uri")?,
        mime_type: row.get("mime_type")?,
        size_bytes: row.get("size_bytes")?,
        sha256: row.get("sha256")?,
        created_at: parse_time(Some(row.get::<_, String>("created_at")?))?.unwrap(),
    })
}

fn parse_time(value: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value
        .map(|s| {
            DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(to_sql_error)
        })
        .transpose()
}

fn to_sql_error<E: std::error::Error + Send + Sync + 'static>(err: E) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(err))
}

fn to_sql_string_error(err: String) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        err,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_echo_runtime;

    #[test]
    fn schema_initializes_and_runtime_round_trips() {
        let db = Storage::open_memory().unwrap();
        let rt = default_echo_runtime();
        db.upsert_runtime(&rt).unwrap();
        assert_eq!(db.list_runtimes().unwrap().len(), 1);
        assert_eq!(
            db.load_runtime("echo").unwrap().unwrap().name,
            "Echo Runtime"
        );
    }

    #[test]
    fn runtime_state_update_works() {
        let db = Storage::open_memory().unwrap();
        db.set_runtime_state("echo", RuntimeStateKind::Ready, None)
            .unwrap();
        assert_eq!(
            db.get_runtime_state("echo").unwrap().state,
            RuntimeStateKind::Ready
        );
    }

    #[test]
    fn file_metadata_and_events_work() {
        let db = Storage::open_memory().unwrap();
        let file = FileRecord {
            id: "f1".to_string(),
            original_name: "a.md".to_string(),
            stored_path: Some("/tmp/a.md".to_string()),
            storage_backend: StorageBackend::Local,
            storage_key: None,
            bucket: None,
            region: None,
            s3_uri: None,
            mime_type: Some("text/markdown".to_string()),
            size_bytes: 4,
            sha256: Some("abcd".to_string()),
            created_at: util::now(),
        };
        db.insert_file(&file).unwrap();
        assert_eq!(db.get_file("f1").unwrap().original_name, "a.md");
        db.record_event(Some("echo"), "wake_requested", None, None)
            .unwrap();
    }
}
