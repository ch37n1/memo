use std::io::Write;
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, Request};
use axum::middleware::Next;
use axum::response::Response;
use memo_core::repositories::TokenRepository;
use memo_core::{ApiError, AuthError, DenialReason, DomainEvent, Token};

use crate::audit::AuditService;
use crate::auth::repository::SqliteTokenRepository;
use crate::HttpError;

#[derive(Debug, Clone)]
pub struct VerifiedToken(pub Token);

#[derive(Debug, Clone)]
pub struct AuthState {
    pub token_repository: Arc<SqliteTokenRepository>,
    pub audit_service: Arc<AuditService>,
}

/// Validates bearer auth and attaches the verified token to request extensions.
///
/// # Errors
///
/// Returns [`ApiError::AuthRequired`] when the header is missing or malformed.
/// Returns auth-related errors from token verification (`token_invalid`,
/// `token_expired`) when verification fails.
pub(crate) async fn auth_middleware(
    State(state): State<AuthState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, HttpError> {
    let token = match extract_bearer_token(request.headers()) {
        Ok(token) => token,
        Err(error) => {
            record_denial(&state.audit_service, DenialReason::AuthRequired).await;
            return Err(error);
        }
    };
    let verified = match state.token_repository.verify(&token).await {
        Ok(token) => token,
        Err(error) => {
            let reason = match error {
                AuthError::AuthRequired => DenialReason::AuthRequired,
                AuthError::TokenInvalid => DenialReason::TokenInvalid,
                AuthError::TokenExpired => DenialReason::TokenExpired,
                AuthError::PermissionDenied => DenialReason::MissingScope,
            };
            record_denial(&state.audit_service, reason).await;
            return Err(HttpError::from(error));
        }
    };
    request.extensions_mut().insert(VerifiedToken(verified));
    Ok(next.run(request).await)
}

async fn record_denial(service: &AuditService, reason: DenialReason) {
    if let Err(error) = service
        .append_domain_event(DomainEvent::AccessDenied {
            token_id: None,
            reason,
            mount: None,
        })
        .await
    {
        let mut stderr = std::io::stderr();
        let _ = writeln!(stderr, "failed to persist auth denial audit entry: {error}");
    }
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<String, HttpError> {
    let header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(HttpError(ApiError::AuthRequired))?;

    let token = header
        .strip_prefix("Bearer ")
        .or_else(|| header.strip_prefix("bearer "))
        .ok_or(HttpError(ApiError::AuthRequired))?;

    if token.trim().is_empty() {
        return Err(HttpError(ApiError::AuthRequired));
    }

    Ok(token.to_owned())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use axum::http::{HeaderMap, HeaderValue};

    use super::extract_bearer_token;

    #[test]
    fn extracts_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer memo_abc"));

        let token = extract_bearer_token(&headers).expect("token should be extracted");
        assert_eq!(token, "memo_abc");
    }
}
