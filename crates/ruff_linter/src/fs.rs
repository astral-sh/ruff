use std::path::{Path, PathBuf};

use log::debug;
use path_absolutize::Absolutize;

use crate::registry::RuleSet;
use crate::settings::types::CompiledPerFileIgnoreList;

/// Create a set with codes matching the pattern/code pairs.
pub(crate) fn ignores_from_path(path: &Path, ignore_list: &CompiledPerFileIgnoreList) -> RuleSet {
    let file_name = path.file_name().expect("Unable to parse filename");
    ignore_list
        .iter()
        .filter_map(|entry| {
            if entry.basename_matcher.is_match(file_name) {
                if entry.negated { None } else {
                    debug!(
                        "Adding per-file ignores for {:?} due to basename match on {:?}: {:?}",
                        path,
                        entry.basename_matcher.glob().regex(),
                        entry.rules
                    );
                    Some(&entry.rules)
                }
            } else if entry.absolute_matcher.is_match(path) {
                if entry.negated { None } else {
                    debug!(
                        "Adding per-file ignores for {:?} due to absolute match on {:?}: {:?}",
                        path,
                        entry.absolute_matcher.glob().regex(),
                        entry.rules
                    );
                    Some(&entry.rules)
                }
            } else if entry.negated {
                debug!(
                    "Adding per-file ignores for {:?} due to negated pattern matching neither {:?} nor {:?}: {:?}",
                    path,
                    entry.basename_matcher.glob().regex(),
                    entry.absolute_matcher.glob().regex(),
                    entry.rules
                );
                Some(&entry.rules)
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

    #[cfg(target_arch = "wasm32")]
    let cwd = Path::new(".");
    #[cfg(not(target_arch = "wasm32"))]
    let cwd = path_absolutize::path_dedot::CWD.as_path();

    if let Ok(path) = path.strip_prefix(cwd) {
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
