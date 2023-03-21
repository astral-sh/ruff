use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use globset::GlobMatcher;
use log::debug;
use path_absolutize::{path_dedot, Absolutize};

use crate::registry::RuleSet;

/// Extract the absolute path and basename (as strings) from a Path.
pub fn extract_path_names(path: &Path) -> Result<(&str, &str)> {
    let file_path = path
        .to_str()
        .ok_or_else(|| anyhow!("Unable to parse filename: {:?}", path))?;
    let file_basename = path
        .file_name()
        .ok_or_else(|| anyhow!("Unable to parse filename: {:?}", path))?
        .to_str()
        .ok_or_else(|| anyhow!("Unable to parse filename: {:?}", path))?;
    Ok((file_path, file_basename))
}

/// Create a set with codes matching the pattern/code pairs.
pub(crate) fn ignores_from_path(
    path: &Path,
    pattern_code_pairs: &[(GlobMatcher, GlobMatcher, RuleSet)],
) -> RuleSet {
    let (file_path, file_basename) = extract_path_names(path).expect("Unable to parse filename");

    pattern_code_pairs
        .iter()
        .filter_map(|(absolute, basename, rules)| {
            if basename.is_match(file_basename) {
                debug!(
                    "Adding per-file ignores for {:?} due to basename match on {:?}: {:?}",
                    path,
                    basename.glob().regex(),
                    rules
                );
                Some(rules)
            } else if absolute.is_match(file_path) {
                debug!(
                    "Adding per-file ignores for {:?} due to absolute match on {:?}: {:?}",
                    path,
                    absolute.glob().regex(),
                    rules
                );
                Some(rules)
            } else {
                None
            }
        })
        .flatten()
        .collect()
}

/// Convert any path to an absolute path (based on the current working
/// directory).
pub fn normalize_path<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if let Ok(path) = path.absolutize() {
        return path.to_path_buf();
    }
    path.to_path_buf()
}

/// Convert any path to an absolute path (based on the specified project root).
pub fn normalize_path_to<P: AsRef<Path>, R: AsRef<Path>>(path: P, project_root: R) -> PathBuf {
    let path = path.as_ref();
    if let Ok(path) = path.absolutize_from(project_root.as_ref()) {
        return path.to_path_buf();
    }
    path.to_path_buf()
}

/// Convert an absolute path to be relative to the current working directory.
pub fn relativize_path<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();
    if let Ok(path) = path.strip_prefix(&*path_dedot::CWD) {
        return format!("{}", path.display());
    }
    format!("{}", path.display())
}

/// Convert an absolute path to be relative to the specified project root.
pub fn relativize_path_to<P: AsRef<Path>, R: AsRef<Path>>(path: P, project_root: R) -> String {
    format!(
        "{}",
        pathdiff::diff_paths(&path, project_root)
            .expect("Could not diff paths")
            .display()
    )
}
