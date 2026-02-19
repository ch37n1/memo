use std::fmt;
use std::path::{Component, Path};
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::PolicyError;
use crate::mount::MountName;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct RelativePath(String);

impl RelativePath {
    /// Creates a normalized relative path.
    ///
    /// # Errors
    /// Returns [`PolicyError::InvalidPath`] for absolute paths, parent traversal,
    /// root/prefix components, or null bytes.
    pub fn new(input: impl AsRef<str>) -> Result<Self, PolicyError> {
        let input = input.as_ref();

        if input.contains('\0') {
            return Err(PolicyError::InvalidPath);
        }

        if input.is_empty() || input == "." {
            return Ok(Self(String::new()));
        }

        let path = Path::new(input);
        if path.is_absolute() {
            return Err(PolicyError::InvalidPath);
        }

        let mut normalized = Vec::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => {
                    normalized.push(segment.to_string_lossy().into_owned());
                }
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(PolicyError::InvalidPath)
                }
            }
        }

        Ok(Self(normalized.join("/")))
    }

    #[must_use]
    pub fn root() -> Self {
        Self(String::new())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_root() {
            f.write_str("/")
        } else {
            f.write_str(self.as_str())
        }
    }
}

impl FromStr for RelativePath {
    type Err = PolicyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for RelativePath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::new(input).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MountPath {
    mount: MountName,
    relative: RelativePath,
}

impl MountPath {
    pub fn new(mount: MountName, relative: RelativePath) -> Self {
        Self { mount, relative }
    }

    /// Parses `<MountName>:/relative/path` into mount and relative components.
    ///
    /// # Errors
    /// Returns [`PolicyError::InvalidPath`] when the format is invalid or when
    /// inner `MountName`/`RelativePath` validation fails.
    pub fn parse(input: impl AsRef<str>) -> Result<Self, PolicyError> {
        let input = input.as_ref();
        let (mount, relative) = input.split_once(":/").ok_or(PolicyError::InvalidPath)?;

        let mount = MountName::new(mount).map_err(|_| PolicyError::InvalidPath)?;
        let relative = RelativePath::new(relative)?;

        Ok(Self::new(mount, relative))
    }

    #[must_use]
    pub fn mount(&self) -> &MountName {
        &self.mount
    }

    #[must_use]
    pub fn relative(&self) -> &RelativePath {
        &self.relative
    }
}

impl fmt::Display for MountPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.relative.is_root() {
            write!(f, "{}:/", self.mount)
        } else {
            write!(f, "{}:/{}", self.mount, self.relative)
        }
    }
}

impl FromStr for MountPath {
    type Err = PolicyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl Serialize for MountPath {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for MountPath {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        Self::parse(input).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use std::str::FromStr;

    use serde_json::json;

    use crate::errors::PolicyError;
    use crate::path::{MountPath, RelativePath};

    #[test]
    fn accepts_valid_relative_paths() {
        assert!(RelativePath::new("notes/git.md").is_ok());
        assert!(RelativePath::new("./notes/./git.md").is_ok());
    }

    #[test]
    fn rejects_absolute_relative_path() {
        assert!(RelativePath::new("/etc/passwd").is_err());
    }

    #[test]
    fn rejects_parent_segments() {
        assert!(RelativePath::new("../secret").is_err());
    }

    #[test]
    fn rejects_null_bytes() {
        assert_eq!(
            RelativePath::new("notes/\0secret"),
            Err(PolicyError::InvalidPath)
        );
    }

    #[test]
    fn root_and_display_are_stable() {
        let root = RelativePath::root();
        assert!(root.is_root());
        assert_eq!(root.to_string(), "/");
        assert_eq!(RelativePath::new(".").expect("dot path should parse"), root);
        assert_eq!(
            RelativePath::from_str("notes/a.md")
                .expect("from_str should parse")
                .to_string(),
            "notes/a.md"
        );
    }

    #[test]
    fn parses_mount_path() {
        let path = MountPath::parse("VaultKB:/notes/git.md").expect("mount path should parse");
        assert_eq!(path.mount().as_str(), "VaultKB");
        assert_eq!(path.relative().as_str(), "notes/git.md");
    }

    #[test]
    fn parses_root_mount_path() {
        let path = MountPath::parse("VaultKB:/").expect("root mount path should parse");
        assert!(path.relative().is_root());
        assert_eq!(path.to_string(), "VaultKB:/");
    }

    #[test]
    fn rejects_invalid_mount_path() {
        assert!(MountPath::parse("VaultKB:notes/git.md").is_err());
        assert!(MountPath::parse("VaultKB://etc/passwd").is_err());
    }

    #[test]
    fn mount_path_from_str_matches_parse() {
        let parsed = MountPath::parse("VaultKB:/notes/git.md").expect("path should parse");
        let from_str = MountPath::from_str("VaultKB:/notes/git.md").expect("from_str should parse");
        assert_eq!(parsed, from_str);
    }

    #[test]
    fn relative_path_deserialize_enforces_validation() {
        let valid: RelativePath =
            serde_json::from_value(json!("notes/a.md")).expect("relative path should deserialize");
        assert_eq!(valid.as_str(), "notes/a.md");

        let invalid = serde_json::from_value::<RelativePath>(json!("../secret"));
        assert!(invalid.is_err());
    }

    #[test]
    fn mount_path_serializes_as_protocol_string() {
        let path = MountPath::parse("VaultKB:/notes/git.md").expect("mount path should parse");
        let json = serde_json::to_value(path).expect("mount path should serialize");
        assert_eq!(json, json!("VaultKB:/notes/git.md"));
    }

    #[test]
    fn mount_path_deserializes_from_protocol_string() {
        let path: MountPath = serde_json::from_value(json!("VaultKB:/notes/git.md"))
            .expect("mount path should deserialize");
        assert_eq!(path.to_string(), "VaultKB:/notes/git.md");
    }
}
