use std::fmt;

use memo_client::MemoClientError;
use memo_core::ApiError;

#[derive(Debug)]
pub enum CliError {
    Config(String),
    Io(String),
    DaemonUnreachable(String),
    Auth(String),
    Permission(String),
    NotFound(String),
    Command(String),
}

impl CliError {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Auth(_) => 2,
            Self::Permission(_) => 3,
            Self::NotFound(_) => 4,
            Self::DaemonUnreachable(_) => 5,
            Self::Config(_) | Self::Io(_) | Self::Command(_) => 1,
        }
    }

    #[must_use]
    pub fn from_client_error(error: MemoClientError) -> Self {
        match error {
            MemoClientError::Api(ApiError::AuthRequired) => {
                Self::Auth("authentication required".to_owned())
            }
            MemoClientError::Api(ApiError::TokenInvalid) => Self::Auth("token invalid".to_owned()),
            MemoClientError::Api(ApiError::TokenExpired) => Self::Auth("token expired".to_owned()),
            MemoClientError::Api(ApiError::PermissionDenied) => {
                Self::Permission("permission denied".to_owned())
            }
            MemoClientError::Api(
                ApiError::NotFound(message) | ApiError::MountNotFound(message),
            ) => Self::NotFound(message),
            MemoClientError::Request(request_error) => {
                Self::DaemonUnreachable(request_error.to_string())
            }
            MemoClientError::Api(api_error) => Self::Command(api_error.to_string()),
            MemoClientError::InvalidBaseUrl(error) => Self::Command(error),
            MemoClientError::Decode(error) => Self::Command(error.to_string()),
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(message)
            | Self::Io(message)
            | Self::DaemonUnreachable(message)
            | Self::Auth(message)
            | Self::Permission(message)
            | Self::NotFound(message)
            | Self::Command(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<toml::de::Error> for CliError {
    fn from(value: toml::de::Error) -> Self {
        Self::Config(value.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use memo_client::MemoClientError;
    use memo_core::ApiError;

    use super::CliError;

    #[test]
    fn exit_codes_match_policy() {
        assert_eq!(CliError::Auth("x".to_owned()).exit_code(), 2);
        assert_eq!(CliError::Permission("x".to_owned()).exit_code(), 3);
        assert_eq!(CliError::NotFound("x".to_owned()).exit_code(), 4);
        assert_eq!(CliError::DaemonUnreachable("x".to_owned()).exit_code(), 5);
        assert_eq!(CliError::Config("x".to_owned()).exit_code(), 1);
        assert_eq!(CliError::Io("x".to_owned()).exit_code(), 1);
        assert_eq!(CliError::Command("x".to_owned()).exit_code(), 1);
    }

    #[test]
    fn client_error_mapping_covers_api_variants() {
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::AuthRequired)),
            CliError::Auth(_)
        ));
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::TokenInvalid)),
            CliError::Auth(_)
        ));
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::TokenExpired)),
            CliError::Auth(_)
        ));
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::PermissionDenied)),
            CliError::Permission(_)
        ));
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::NotFound(
                "missing".to_owned()
            ))),
            CliError::NotFound(_)
        ));
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::MountNotFound(
                "missing mount".to_owned()
            ))),
            CliError::NotFound(_)
        ));
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Api(ApiError::Conflict(
                "conflict".to_owned()
            ))),
            CliError::Command(_)
        ));
    }

    #[test]
    fn client_error_mapping_covers_non_api_variants() {
        assert!(matches!(
            CliError::from_client_error(MemoClientError::InvalidBaseUrl("bad".to_owned())),
            CliError::Command(_)
        ));
        let decode_error =
            serde_json::from_str::<serde_json::Value>("not json").expect_err("must fail decode");
        assert!(matches!(
            CliError::from_client_error(MemoClientError::Decode(decode_error)),
            CliError::Command(_)
        ));
    }

    #[test]
    fn display_and_from_conversions_round_trip_message() {
        let io = CliError::from(std::io::Error::other("io issue"));
        assert!(matches!(io, CliError::Io(_)));
        assert!(io.to_string().contains("io issue"));

        let toml_err = toml::from_str::<toml::Value>("=").expect_err("invalid toml");
        let cfg = CliError::from(toml_err);
        assert!(matches!(cfg, CliError::Config(_)));
        assert!(!cfg.to_string().is_empty());
    }
}
