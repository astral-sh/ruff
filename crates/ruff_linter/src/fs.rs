use std::path::{Path, PathBuf};

use path_absolutize::Absolutize;

use crate::registry::RuleSet;
use crate::settings::types::CompiledPerFileIgnoreList;

/// Return the current working directory.
///
/// On WASM this just returns `.`. Otherwise, defer to [`path_absolutize::path_dedot::CWD`].
pub fn get_cwd() -> &'static Path {
    #[cfg(target_arch = "wasm32")]
    {
        static CWD: std::sync::LazyLock<PathBuf> = std::sync::LazyLock::new(|| PathBuf::from("."));
        &CWD
    }
    #[cfg(not(target_arch = "wasm32"))]
    path_absolutize::path_dedot::CWD.as_path()
}

/// Create a set with codes matching the pattern/code pairs.
pub(crate) fn ignores_from_path(path: &Path, ignore_list: &CompiledPerFileIgnoreList) -> RuleSet {
    ignore_list
        .iter_matches(path, "Adding per-file ignores")
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

    let cwd = get_cwd();
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
