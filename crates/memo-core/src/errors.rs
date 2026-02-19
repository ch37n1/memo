use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PolicyError {
    #[error("invalid path")]
    InvalidPath,
    #[error("invalid policy configuration: {0}")]
    InvalidPolicy(String),
    #[error("path escapes mount root")]
    OutOfBounds,
    #[error("symlinks are denied")]
    SymlinkDenied,
    #[error("path not found")]
    NotFound,
    #[error("permission denied by policy")]
    PermissionDenied,
    #[error("file too large: {actual} > {limit}")]
    TooLarge { limit: u64, actual: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DbError {
    #[error("record not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("database query failed: {0}")]
    Query(String),
    #[error("database connection failed: {0}")]
    Connection(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthError {
    #[error("authentication required")]
    AuthRequired,
    #[error("token invalid")]
    TokenInvalid,
    #[error("token expired")]
    TokenExpired,
    #[error("permission denied")]
    PermissionDenied,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ApiError {
    #[error("auth required")]
    AuthRequired,
    #[error("token invalid")]
    TokenInvalid,
    #[error("token expired")]
    TokenExpired,
    #[error("permission denied")]
    PermissionDenied,
    #[error("policy violated: {0}")]
    PolicyViolated(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("path escapes mount root")]
    OutOfBounds,
    #[error("symlink denied")]
    SymlinkDenied,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("mount not found: {0}")]
    MountNotFound(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("too large: {actual} > {limit}")]
    TooLarge { limit: u64, actual: u64 },
    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::AuthRequired => "auth_required",
            Self::TokenInvalid => "token_invalid",
            Self::TokenExpired => "token_expired",
            Self::PermissionDenied => "permission_denied",
            Self::PolicyViolated(_) => "policy_violated",
            Self::InvalidPath(_) => "invalid_path",
            Self::OutOfBounds => "out_of_bounds",
            Self::SymlinkDenied => "symlink_denied",
            Self::NotFound(_) => "not_found",
            Self::MountNotFound(_) => "mount_not_found",
            Self::Conflict(_) => "conflict",
            Self::TooLarge { .. } => "too_large",
            Self::Internal(_) => "internal_error",
        }
    }
}

impl From<PolicyError> for ApiError {
    fn from(value: PolicyError) -> Self {
        match value {
            PolicyError::InvalidPath => Self::InvalidPath("invalid path".to_owned()),
            PolicyError::InvalidPolicy(reason) => {
                Self::Internal(format!("invalid policy configuration: {reason}"))
            }
            PolicyError::OutOfBounds => Self::OutOfBounds,
            PolicyError::SymlinkDenied => Self::SymlinkDenied,
            PolicyError::NotFound => Self::NotFound("path not found".to_owned()),
            PolicyError::PermissionDenied => Self::PolicyViolated("permission denied".to_owned()),
            PolicyError::TooLarge { limit, actual } => Self::TooLarge { limit, actual },
        }
    }
}

impl From<DbError> for ApiError {
    fn from(value: DbError) -> Self {
        match value {
            DbError::NotFound => Self::NotFound("record not found".to_owned()),
            DbError::Conflict => Self::Conflict("conflict".to_owned()),
            DbError::Query(error) | DbError::Connection(error) => Self::Internal(error),
        }
    }
}

impl From<AuthError> for ApiError {
    fn from(value: AuthError) -> Self {
        match value {
            AuthError::AuthRequired => Self::AuthRequired,
            AuthError::TokenInvalid => Self::TokenInvalid,
            AuthError::TokenExpired => Self::TokenExpired,
            AuthError::PermissionDenied => Self::PermissionDenied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApiError, AuthError, DbError, PolicyError};

    #[test]
    fn maps_policy_error_to_api_error() {
        let error: ApiError = PolicyError::OutOfBounds.into();
        assert_eq!(error.code(), "out_of_bounds");
    }

    #[test]
    fn maps_db_error_to_api_error() {
        let error: ApiError = DbError::Conflict.into();
        assert_eq!(error.code(), "conflict");
    }

    #[test]
    fn api_error_code_covers_variants() {
        let cases = [
            (ApiError::AuthRequired, "auth_required"),
            (ApiError::TokenInvalid, "token_invalid"),
            (ApiError::TokenExpired, "token_expired"),
            (ApiError::PermissionDenied, "permission_denied"),
            (ApiError::PolicyViolated("x".to_owned()), "policy_violated"),
            (ApiError::InvalidPath("x".to_owned()), "invalid_path"),
            (ApiError::OutOfBounds, "out_of_bounds"),
            (ApiError::SymlinkDenied, "symlink_denied"),
            (ApiError::NotFound("x".to_owned()), "not_found"),
            (ApiError::MountNotFound("x".to_owned()), "mount_not_found"),
            (ApiError::Conflict("x".to_owned()), "conflict"),
            (
                ApiError::TooLarge {
                    limit: 1,
                    actual: 2,
                },
                "too_large",
            ),
            (ApiError::Internal("x".to_owned()), "internal_error"),
        ];

        for (error, expected_code) in cases {
            assert_eq!(error.code(), expected_code);
        }
    }

    #[test]
    fn maps_auth_error_to_api_error() {
        assert_eq!(
            ApiError::from(AuthError::AuthRequired),
            ApiError::AuthRequired
        );
        assert_eq!(
            ApiError::from(AuthError::TokenInvalid),
            ApiError::TokenInvalid
        );
        assert_eq!(
            ApiError::from(AuthError::TokenExpired),
            ApiError::TokenExpired
        );
        assert_eq!(
            ApiError::from(AuthError::PermissionDenied),
            ApiError::PermissionDenied
        );
    }

    #[test]
    fn maps_policy_error_variants() {
        assert_eq!(
            ApiError::from(PolicyError::InvalidPath),
            ApiError::InvalidPath("invalid path".to_owned())
        );
        assert_eq!(
            ApiError::from(PolicyError::InvalidPolicy("bad glob".to_owned())),
            ApiError::Internal("invalid policy configuration: bad glob".to_owned())
        );
        assert_eq!(
            ApiError::from(PolicyError::SymlinkDenied),
            ApiError::SymlinkDenied
        );
        assert_eq!(
            ApiError::from(PolicyError::NotFound),
            ApiError::NotFound("path not found".to_owned())
        );
        assert_eq!(
            ApiError::from(PolicyError::PermissionDenied),
            ApiError::PolicyViolated("permission denied".to_owned())
        );
        assert_eq!(
            ApiError::from(PolicyError::TooLarge {
                limit: 8,
                actual: 9
            }),
            ApiError::TooLarge {
                limit: 8,
                actual: 9
            }
        );
    }

    #[test]
    fn maps_db_error_variants() {
        assert_eq!(
            ApiError::from(DbError::NotFound),
            ApiError::NotFound("record not found".to_owned())
        );
        assert_eq!(
            ApiError::from(DbError::Query("q".to_owned())),
            ApiError::Internal("q".to_owned())
        );
        assert_eq!(
            ApiError::from(DbError::Connection("c".to_owned())),
            ApiError::Internal("c".to_owned())
        );
    }
}
