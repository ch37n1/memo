use std::path::Path;

use regex::RegexBuilder;
use walkdir::WalkDir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrepHit {
    pub relative_path: String,
    pub line: u64,
    pub content: String,
}

/// Searches text files under `start` for regex pattern matches.
///
/// # Errors
///
/// Returns an error when regex parsing fails.
pub fn grep(
    root: &Path,
    start: &Path,
    pattern: &str,
    recursive: bool,
    case_sensitive: bool,
    max_results: usize,
) -> Result<Vec<GrepHit>, regex::Error> {
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()?;

    let mut results = Vec::new();
    let mut walker = WalkDir::new(start).follow_links(false);
    if !recursive {
        walker = walker.max_depth(1);
    }

    for entry in walker.into_iter().filter_map(Result::ok) {
        if results.len() >= max_results {
            break;
        }

        let path = entry.path();
        if !entry.file_type().is_file() {
            continue;
        }

        let Ok(data) = std::fs::read(path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(data) else {
            continue;
        };

        for (idx, line) in text.lines().enumerate() {
            if regex.is_match(line) {
                let relative = path
                    .strip_prefix(root)
                    .ok()
                    .map(|v| v.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                results.push(GrepHit {
                    relative_path: relative,
                    line: idx as u64 + 1,
                    content: line.to_owned(),
                });

                if results.len() >= max_results {
                    break;
                }
            }
        }
    }

    Ok(results)
}
