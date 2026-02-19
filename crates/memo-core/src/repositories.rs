use async_trait::async_trait;

use crate::errors::{AuthError, DbError};
use crate::mount::{Mount, MountName};
use crate::token::{Token, TokenId, TokenView};

#[async_trait]
pub trait MountRepository: Send + Sync {
    async fn find(&self, name: &MountName) -> Result<Mount, DbError>;
    async fn list(&self) -> Result<Vec<Mount>, DbError>;
    async fn save(&self, mount: &Mount) -> Result<(), DbError>;
    async fn delete(&self, name: &MountName) -> Result<(), DbError>;
}

#[async_trait]
pub trait TokenRepository: Send + Sync {
    async fn find(&self, id: &TokenId) -> Result<Token, DbError>;
    async fn verify(&self, raw_token: &str) -> Result<Token, AuthError>;
    async fn list(&self) -> Result<Vec<TokenView>, DbError>;
    async fn save(&self, token: &Token) -> Result<(), DbError>;
    async fn delete(&self, id: &TokenId) -> Result<(), DbError>;
    async fn touch_last_used(&self, id: &TokenId) -> Result<(), DbError>;
}
