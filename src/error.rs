use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use thiserror::Error;

use crate::model::{ApiErrorBody, ApiErrorDetails};

#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum EmberlaneError {
    #[error("runtime not found: {0}")]
    RuntimeNotFound(String),
    #[error("runtime is disabled: {0}")]
    RuntimeDisabled(String),
    #[error("runtime is warming: {0}")]
    RuntimeWarming(String),
    #[error("wake failed: {0}")]
    WakeFailed(String),
    #[error("sleep failed: {0}")]
    SleepFailed(String),
    #[error("provider is not configured: {0}")]
    ProviderNotConfigured(String),
    #[error("provider operation is not implemented: {0}")]
    ProviderNotImplemented(String),
    #[error("route failed: {0}")]
    RouteFailed(String),
    #[error("health check failed: {0}")]
    HealthCheckFailed(String),
    #[error("max concurrency exceeded")]
    MaxConcurrencyExceeded,
    #[error("authorization required")]
    AuthRequired,
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("request too large")]
    RequestTooLarge,
    #[error("rate limited")]
    RateLimited,
    #[error("storage is not configured: {0}")]
    StorageNotConfigured(String),
    #[error("storage backend is not supported: {0}")]
    StorageBackendUnsupported(String),
    #[error("S3 upload failed: {0}")]
    S3UploadFailed(String),
    #[error("S3 download failed: {0}")]
    S3DownloadFailed(String),
    #[error("S3 presign failed: {0}")]
    S3PresignFailed(String),
    #[error("presigned URLs are only supported for S3-backed files")]
    PresignNotSupported,
    #[error("unsafe file name: {0}")]
    UnsafeFileName(String),
    #[error(
        "local files cannot be routed to aws_asg runtimes. Use S3 storage or upload the file to S3."
    )]
    LocalFileNotAvailableToRemoteRuntime,
    #[error("internal error: {0}")]
    Internal(String),
}

impl EmberlaneError {
    pub fn code(&self) -> &'static str {
        match self {
            EmberlaneError::RuntimeNotFound(_) => "runtime_not_found",
            EmberlaneError::RuntimeDisabled(_) => "runtime_disabled",
            EmberlaneError::RuntimeWarming(_) => "runtime_warming",
            EmberlaneError::WakeFailed(_) => "wake_failed",
            EmberlaneError::SleepFailed(_) => "sleep_failed",
            EmberlaneError::ProviderNotConfigured(_) => "provider_not_configured",
            EmberlaneError::ProviderNotImplemented(_) => "provider_not_implemented",
            EmberlaneError::RouteFailed(_) => "route_failed",
            EmberlaneError::HealthCheckFailed(_) => "health_check_failed",
            EmberlaneError::MaxConcurrencyExceeded => "max_concurrency_exceeded",
            EmberlaneError::AuthRequired => "auth_required",
            EmberlaneError::InvalidRequest(_) => "invalid_request",
            EmberlaneError::FileNotFound(_) => "file_not_found",
            EmberlaneError::RequestTooLarge => "request_too_large",
            EmberlaneError::RateLimited => "rate_limited",
            EmberlaneError::StorageNotConfigured(_) => "storage_not_configured",
            EmberlaneError::StorageBackendUnsupported(_) => "storage_backend_unsupported",
            EmberlaneError::S3UploadFailed(_) => "s3_upload_failed",
            EmberlaneError::S3DownloadFailed(_) => "s3_download_failed",
            EmberlaneError::S3PresignFailed(_) => "s3_presign_failed",
            EmberlaneError::PresignNotSupported => "presign_not_supported",
            EmberlaneError::UnsafeFileName(_) => "unsafe_file_name",
            EmberlaneError::LocalFileNotAvailableToRemoteRuntime => {
                "local_file_not_available_to_remote_runtime"
            }
            EmberlaneError::Internal(_) => "internal_error",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            EmberlaneError::RuntimeNotFound(_) | EmberlaneError::FileNotFound(_) => {
                StatusCode::NOT_FOUND
            }
            EmberlaneError::RuntimeDisabled(_) => StatusCode::FORBIDDEN,
            EmberlaneError::AuthRequired => StatusCode::UNAUTHORIZED,
            EmberlaneError::MaxConcurrencyExceeded | EmberlaneError::RateLimited => {
                StatusCode::TOO_MANY_REQUESTS
            }
            EmberlaneError::RequestTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            EmberlaneError::InvalidRequest(_)
            | EmberlaneError::StorageNotConfigured(_)
            | EmberlaneError::StorageBackendUnsupported(_)
            | EmberlaneError::PresignNotSupported
            | EmberlaneError::UnsafeFileName(_)
            | EmberlaneError::LocalFileNotAvailableToRemoteRuntime => StatusCode::BAD_REQUEST,
            EmberlaneError::WakeFailed(_)
            | EmberlaneError::RouteFailed(_)
            | EmberlaneError::HealthCheckFailed(_)
            | EmberlaneError::S3UploadFailed(_)
            | EmberlaneError::S3DownloadFailed(_)
            | EmberlaneError::S3PresignFailed(_) => StatusCode::BAD_GATEWAY,
            EmberlaneError::RuntimeWarming(_) => StatusCode::ACCEPTED,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<anyhow::Error> for EmberlaneError {
    fn from(value: anyhow::Error) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<rusqlite::Error> for EmberlaneError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<std::io::Error> for EmberlaneError {
    fn from(value: std::io::Error) -> Self {
        Self::Internal(value.to_string())
    }
}

impl From<reqwest::Error> for EmberlaneError {
    fn from(value: reqwest::Error) -> Self {
        Self::RouteFailed(value.to_string())
    }
}

impl IntoResponse for EmberlaneError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status();
        let body = ApiErrorBody {
            ok: false,
            error: ApiErrorDetails {
                code: self.code().to_string(),
                message: self.to_string(),
                details: json!({}),
            },
        };
        (status, Json(body)).into_response()
    }
}
