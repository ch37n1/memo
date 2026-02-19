use std::path::Path;

use globset::{Glob, GlobMatcher};
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindHit {
    pub relative_path: String,
}

/// Finds paths whose file name matches a glob.
///
/// # Errors
///
/// Returns an error when the glob pattern is invalid.
pub fn find(
    root: &Path,
    start: &Path,
    glob: &str,
    max_results: usize,
) -> Result<Vec<FindHit>, globset::Error> {
    let matcher: GlobMatcher = Glob::new(glob)?.compile_matcher();
    let mut results = Vec::new();

    for entry in WalkDir::new(start)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
    {
        if results.len() >= max_results {
            break;
        }

        let path = entry.path();
        let file_name = path
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("");
        if !matcher.is_match(file_name) {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .ok()
            .map(|v| v.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        results.push(FindHit {
            relative_path: relative,
        });
    }

    Ok(results)
}
