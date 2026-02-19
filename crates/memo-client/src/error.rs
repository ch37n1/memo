use memo_core::ApiError;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct ErrorResponse {
    pub error: ErrorEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub(crate) struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub mount: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub actual: Option<u64>,
}

#[derive(Debug, Error)]
pub enum MemoClientError {
    #[error("invalid base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("decode failed: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("api error: {0}")]
    Api(ApiError),
}

impl MemoClientError {
    #[must_use]
    pub fn as_api_error(&self) -> Option<&ApiError> {
        match self {
            Self::Api(error) => Some(error),
            _ => None,
        }
    }
}

pub(crate) fn map_api_error(error: ErrorEnvelope) -> ApiError {
    let contextual_message = match (&error.mount, &error.path) {
        (Some(mount), Some(path)) => format!("{} (mount: {mount}, path: {path})", error.message),
        (Some(mount), None) => format!("{} (mount: {mount})", error.message),
        (None, Some(path)) => format!("{} (path: {path})", error.message),
        (None, None) => error.message.clone(),
    };

    match error.code.as_str() {
        "auth_required" => ApiError::AuthRequired,
        "token_invalid" => ApiError::TokenInvalid,
        "token_expired" => ApiError::TokenExpired,
        "permission_denied" => ApiError::PermissionDenied,
        "policy_violated" => ApiError::PolicyViolated(error.message),
        "invalid_path" => ApiError::InvalidPath(contextual_message),
        "out_of_bounds" => ApiError::OutOfBounds,
        "symlink_denied" => ApiError::SymlinkDenied,
        "not_found" => ApiError::NotFound(contextual_message),
        "mount_not_found" => ApiError::MountNotFound(contextual_message),
        "conflict" => ApiError::Conflict(contextual_message),
        "too_large" => ApiError::TooLarge {
            limit: error.limit.unwrap_or(0),
            actual: error.actual.unwrap_or(0),
        },
        _ => ApiError::Internal(error.message),
    }
}
