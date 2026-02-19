use std::fmt;

use memo_client::MemoClientError;
use memo_core::ApiError;

#[derive(Debug)]
pub enum CliError {
    Config(String),
    Io(String),
    DaemonUnreachable(String),
    Auth(String),
    Command(String),
}

impl CliError {
    #[must_use]
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Auth(_) => 2,
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
