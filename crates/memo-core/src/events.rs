use serde::{Deserialize, Serialize};

use crate::mount::{MountMode, MountName};
use crate::path::RelativePath;
use crate::token::TokenId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DenialReason {
    AuthRequired,
    TokenInvalid,
    TokenExpired,
    MissingScope,
    PolicyViolation,
    InvalidPath,
    OutOfBounds,
    SymlinkDenied,
    MountNotFound,
    NotFound,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    FileRead {
        token_id: TokenId,
        mount: MountName,
        path: RelativePath,
        bytes: u64,
    },
    FileWritten {
        token_id: TokenId,
        mount: MountName,
        path: RelativePath,
        bytes: u64,
    },
    DirListed {
        token_id: TokenId,
        mount: MountName,
        path: RelativePath,
    },
    MountRegistered {
        name: MountName,
        mode: MountMode,
    },
    MountUpdated {
        name: MountName,
    },
    MountRemoved {
        name: MountName,
    },
    TokenCreated {
        id: TokenId,
        name: String,
    },
    TokenRevoked {
        id: TokenId,
    },
    AccessDenied {
        token_id: Option<TokenId>,
        reason: DenialReason,
        mount: Option<MountName>,
    },
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::events::DomainEvent;
    use crate::mount::{MountMode, MountName};

    #[test]
    fn serializes_domain_event() {
        let event = DomainEvent::MountRegistered {
            name: MountName::new("VaultKB").expect("mount name should parse"),
            mode: MountMode::ReadWrite,
        };

        let json = serde_json::to_string(&event).expect("event should serialize");
        assert!(json.contains("mount_registered"));
    }
}
