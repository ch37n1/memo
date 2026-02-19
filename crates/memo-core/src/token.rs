use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::scope::{Scope, ScopeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenId(Uuid);

impl TokenId {
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    #[must_use]
    pub fn from_uuid(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn into_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for TokenId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expiry {
    Never,
    At(OffsetDateTime),
}

impl Expiry {
    #[must_use]
    pub fn is_expired_at(&self, now: OffsetDateTime) -> bool {
        match self {
            Self::Never => false,
            Self::At(expires_at) => now >= *expires_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub id: TokenId,
    pub name: String,
    pub hash: String,
    pub scopes: ScopeSet,
    pub created_at: OffsetDateTime,
    pub expires_at: Expiry,
    pub last_used_at: Option<OffsetDateTime>,
}

impl Token {
    /// Creates a validated token aggregate.
    ///
    /// # Errors
    /// Returns [`TokenError`] when `name` or `hash` are empty, or when scopes are empty.
    pub fn new(
        id: TokenId,
        name: impl Into<String>,
        hash: impl Into<String>,
        scopes: ScopeSet,
        created_at: OffsetDateTime,
        expires_at: Expiry,
    ) -> Result<Self, TokenError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(TokenError::InvalidName);
        }

        let hash = hash.into();
        if hash.trim().is_empty() {
            return Err(TokenError::InvalidHash);
        }

        if scopes.is_empty() {
            return Err(TokenError::EmptyScopes);
        }

        Ok(Self {
            id,
            name,
            hash,
            scopes,
            created_at,
            expires_at,
            last_used_at: None,
        })
    }

    #[must_use]
    pub fn has_scope(&self, required: &Scope) -> bool {
        self.scopes.contains_required(required)
    }

    #[must_use]
    pub fn is_expired_at(&self, now: OffsetDateTime) -> bool {
        self.expires_at.is_expired_at(now)
    }

    #[must_use]
    pub fn mark_used(mut self, now: OffsetDateTime) -> Self {
        self.last_used_at = Some(now);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenView {
    pub id: TokenId,
    pub name: String,
    pub scopes: ScopeSet,
    pub created_at: OffsetDateTime,
    pub expires_at: Expiry,
    pub last_used_at: Option<OffsetDateTime>,
}

impl From<&Token> for TokenView {
    fn from(token: &Token) -> Self {
        Self {
            id: token.id,
            name: token.name.clone(),
            scopes: token.scopes.clone(),
            created_at: token.created_at,
            expires_at: token.expires_at.clone(),
            last_used_at: token.last_used_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("token name cannot be empty")]
    InvalidName,
    #[error("token hash cannot be empty")]
    InvalidHash,
    #[error("token must have at least one scope")]
    EmptyScopes,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use time::{Duration, OffsetDateTime};

    use crate::scope::{Scope, ScopeSet};
    use crate::token::{Expiry, Token, TokenError, TokenId};

    #[test]
    fn token_has_scope() {
        let now = OffsetDateTime::now_utc();
        let token = Token::new(
            TokenId::new(),
            "agent",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::new([Scope::from_str("fs:*:read").expect("scope should parse")]),
            now,
            Expiry::Never,
        )
        .expect("token should be valid");

        let required = Scope::from_str("fs:VaultKB:read").expect("scope should parse");
        assert!(token.has_scope(&required));
    }

    #[test]
    fn token_expiry_is_checked() {
        let now = OffsetDateTime::now_utc();
        let token = Token::new(
            TokenId::new(),
            "agent",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::new([Scope::from_str("fs:*:read").expect("scope should parse")]),
            now,
            Expiry::At(now - Duration::minutes(1)),
        )
        .expect("token should be valid");

        assert!(token.is_expired_at(now));
    }

    #[test]
    fn token_requires_scopes() {
        let now = OffsetDateTime::now_utc();
        let result = Token::new(
            TokenId::new(),
            "agent",
            "$argon2id$v=19$m=19456,t=2,p=1$abc$xyz",
            ScopeSet::default(),
            now,
            Expiry::Never,
        );

        assert_eq!(result, Err(TokenError::EmptyScopes));
    }
}
