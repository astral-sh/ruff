use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use globset::GlobMatcher;
use path_absolutize::{path_dedot, Absolutize};
use rustc_hash::FxHashSet;

use crate::checks::CheckCode;

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
pub(crate) fn ignores_from_path<'a>(
    path: &Path,
    pattern_code_pairs: &'a [(GlobMatcher, GlobMatcher, FxHashSet<CheckCode>)],
) -> Result<FxHashSet<&'a CheckCode>> {
    let (file_path, file_basename) = extract_path_names(path)?;
    Ok(pattern_code_pairs
        .iter()
        .filter(|(absolute, basename, _)| {
            basename.is_match(file_basename) || absolute.is_match(file_path)
        })
        .flat_map(|(_, _, codes)| codes)
        .collect())
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
pub fn relativize_path(path: &Path) -> Cow<str> {
    if let Ok(path) = path.strip_prefix(&*path_dedot::CWD) {
        return path.to_string_lossy();
    }
    path.to_string_lossy()
}

/// Read a file's contents from disk.
pub(crate) fn read_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}
