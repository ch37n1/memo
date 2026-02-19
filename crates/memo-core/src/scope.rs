use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::mount::MountName;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Scope {
    Fs { mount: ScopeMount, action: FsAction },
    Meta(MetaScope),
    Admin(AdminScope),
}

impl Scope {
    #[must_use]
    pub fn matches(&self, required: &Self) -> bool {
        if self == required {
            return true;
        }

        match (self, required) {
            (
                Self::Fs {
                    mount: ScopeMount::Any,
                    action: granted,
                },
                Self::Fs { action, .. },
            ) => granted == action,
            (Self::Admin(AdminScope::All), Self::Admin(_)) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fs { mount, action } => write!(f, "fs:{mount}:{action}"),
            Self::Meta(scope) => write!(f, "meta:{scope}"),
            Self::Admin(scope) => write!(f, "admin:{scope}"),
        }
    }
}

impl FromStr for Scope {
    type Err = ScopeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = input.split(':').collect();
        if parts.len() != 3 {
            return Err(ScopeParseError::InvalidFormat);
        }

        match parts[0] {
            "fs" => {
                let mount = ScopeMount::from_str(parts[1])?;
                let action = FsAction::from_str(parts[2])?;
                Ok(Self::Fs { mount, action })
            }
            "meta" => {
                let scope = MetaScope::from_str(parts[2])?;
                if parts[1] != "*" {
                    return Err(ScopeParseError::InvalidFormat);
                }
                Ok(Self::Meta(scope))
            }
            "admin" => {
                let scope = AdminScope::from_str(parts[2])?;
                if parts[1] != "*" {
                    return Err(ScopeParseError::InvalidFormat);
                }
                Ok(Self::Admin(scope))
            }
            _ => Err(ScopeParseError::UnknownNamespace),
        }
    }
}

impl Serialize for Scope {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Scope {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::from_str(&input).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScopeMount {
    Any,
    Named(MountName),
}

impl fmt::Display for ScopeMount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Any => f.write_str("*"),
            Self::Named(name) => write!(f, "{name}"),
        }
    }
}

impl FromStr for ScopeMount {
    type Err = ScopeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        if input == "*" {
            return Ok(Self::Any);
        }

        let mount = MountName::new(input).map_err(|_| ScopeParseError::InvalidMountName)?;
        Ok(Self::Named(mount))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FsAction {
    Read,
    Write,
}

impl fmt::Display for FsAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => f.write_str("read"),
            Self::Write => f.write_str("write"),
        }
    }
}

impl FromStr for FsAction {
    type Err = ScopeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            _ => Err(ScopeParseError::UnknownAction),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdminScope {
    Mounts,
    Tokens,
    All,
}

impl fmt::Display for AdminScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mounts => f.write_str("mounts"),
            Self::Tokens => f.write_str("tokens"),
            Self::All => f.write_str("*"),
        }
    }
}

impl FromStr for AdminScope {
    type Err = ScopeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "mounts" => Ok(Self::Mounts),
            "tokens" => Ok(Self::Tokens),
            "*" => Ok(Self::All),
            _ => Err(ScopeParseError::UnknownAction),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MetaScope {
    Read,
}

impl fmt::Display for MetaScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => f.write_str("read"),
        }
    }
}

impl FromStr for MetaScope {
    type Err = ScopeParseError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
            "read" => Ok(Self::Read),
            _ => Err(ScopeParseError::UnknownAction),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScopeParseError {
    #[error("invalid scope format")]
    InvalidFormat,
    #[error("unknown scope namespace")]
    UnknownNamespace,
    #[error("unknown scope action")]
    UnknownAction,
    #[error("invalid mount name")]
    InvalidMountName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct ScopeSet(BTreeSet<Scope>);

impl ScopeSet {
    #[must_use]
    pub fn new(scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self(scopes.into_iter().collect())
    }

    #[must_use]
    pub fn contains_required(&self, required: &Scope) -> bool {
        self.0.iter().any(|granted| granted.matches(required))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn insert(&mut self, scope: Scope) {
        let _ = self.0.insert(scope);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Scope> {
        self.0.iter()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use crate::scope::{FsAction, Scope, ScopeMount, ScopeSet};

    #[test]
    fn parses_fs_scope() {
        let scope = Scope::from_str("fs:VaultKB:read").expect("scope should parse");
        assert_eq!(
            scope,
            Scope::Fs {
                mount: ScopeMount::Named(
                    crate::mount::MountName::new("VaultKB").expect("mount name should parse"),
                ),
                action: FsAction::Read,
            }
        );
    }

    #[test]
    fn wildcard_matches_required_fs_scope() {
        let granted = Scope::from_str("fs:*:read").expect("scope should parse");
        let required = Scope::from_str("fs:VaultKB:read").expect("scope should parse");
        assert!(granted.matches(&required));
    }

    #[test]
    fn fs_read_does_not_match_write() {
        let granted = Scope::from_str("fs:*:read").expect("scope should parse");
        let required = Scope::from_str("fs:VaultKB:write").expect("scope should parse");
        assert!(!granted.matches(&required));
    }

    #[test]
    fn admin_star_matches_admin_scope() {
        let granted = Scope::from_str("admin:*:*").expect("scope should parse");
        let required = Scope::from_str("admin:*:tokens").expect("scope should parse");
        assert!(granted.matches(&required));
    }

    #[test]
    fn scope_set_checks_required_scope() {
        let scopes = ScopeSet::new([
            Scope::from_str("fs:*:read").expect("scope should parse"),
            Scope::from_str("meta:*:read").expect("scope should parse"),
        ]);

        let required = Scope::from_str("fs:VaultKB:read").expect("scope should parse");
        assert!(scopes.contains_required(&required));
    }
}
