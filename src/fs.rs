use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use globset::GlobMatcher;
use log::debug;
use path_absolutize::{path_dedot, Absolutize};
use rustc_hash::FxHashSet;
use walkdir::{DirEntry, WalkDir};

use crate::checks::CheckCode;
use crate::resolver::Resolver;

/// Extract the absolute path and basename (as strings) from a Path.
fn extract_path_names(path: &Path) -> Result<(&str, &str)> {
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

fn is_excluded(file_path: &str, file_basename: &str, exclude: &globset::GlobSet) -> bool {
    exclude.is_match(file_path) || exclude.is_match(file_basename)
}

fn is_included(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "py" || ext == "pyi")
}

/// Find all `pyproject.toml` files for a given `Path`. Both parents and
/// children will be included in the resulting `Vec`.
pub fn iter_pyproject_files(path: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Search for `pyproject.toml` files in all parent directories.
    let path = normalize_path(path);
    for path in path.ancestors() {
        if path.is_dir() {
            let toml_path = path.join("pyproject.toml");
            if toml_path.exists() {
                paths.push(toml_path);
            }
        }
    }

    // Search for `pyproject.toml` files in all child directories.
    for path in WalkDir::new(path)
        .into_iter()
        .filter_entry(|entry| {
            entry.file_name().to_str().map_or(false, |file_name| {
                entry.depth() == 0 || !file_name.starts_with('.')
            })
        })
        .filter_map(std::result::Result::ok)
        .filter(|entry| entry.path().ends_with("pyproject.toml"))
    {
        paths.push(path.into_path());
    }

    paths
}

/// Find all Python (`.py` and `.pyi` files) in a given `Path`.
pub fn iter_python_files<'a>(
    path: &'a Path,
    resolver: &'a Resolver<'a>,
) -> impl Iterator<Item = Result<DirEntry, walkdir::Error>> + 'a {
    WalkDir::new(normalize_path(path))
        .into_iter()
        .filter_entry(move |entry| {
            let path = entry.path();
            let settings = resolver.resolve(path);
            let exclude = &settings.exclude;
            let extend_exclude = &settings.extend_exclude;

            match extract_path_names(path) {
                Ok((file_path, file_basename)) => {
                    if !exclude.is_empty() && is_excluded(file_path, file_basename, exclude) {
                        debug!("Ignored path via `exclude`: {:?}", path);
                        false
                    } else if !extend_exclude.is_empty()
                        && is_excluded(file_path, file_basename, extend_exclude)
                    {
                        debug!("Ignored path via `extend-exclude`: {:?}", path);
                        false
                    } else {
                        true
                    }
                }
                Err(e) => {
                    debug!("Ignored path due to error in parsing: {:?}: {}", path, e);
                    true
                }
            }
        })
        .filter(|entry| {
            entry.as_ref().map_or(true, |entry| {
                (entry.depth() == 0 || is_included(entry.path()))
                    && !entry.file_type().is_dir()
                    && !(entry.file_type().is_symlink() && entry.path().is_dir())
            })
        })
}

/// Create tree set with codes matching the pattern/code pairs.
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
pub fn normalize_path(path: &Path) -> PathBuf {
    if let Ok(path) = path.absolutize() {
        return path.to_path_buf();
    }
    path.to_path_buf()
}

/// Convert any path to an absolute path (based on the specified project root).
pub fn normalize_path_to(path: &Path, project_root: &Path) -> PathBuf {
    if let Ok(path) = path.absolutize_from(project_root) {
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
pub(crate) fn read_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);
    let mut contents = String::new();
    buf_reader.read_to_string(&mut contents)?;
    Ok(contents)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use globset::GlobSet;
    use path_absolutize::Absolutize;

    use crate::fs::{extract_path_names, is_excluded, is_included};
    use crate::settings::types::FilePattern;

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        assert!(is_included(&path));

        let path = Path::new("foo/bar/baz.pyi").absolutize().unwrap();
        assert!(is_included(&path));

        let path = Path::new("foo/bar/baz.js").absolutize().unwrap();
        assert!(!is_included(&path));

        let path = Path::new("foo/bar/baz").absolutize().unwrap();
        assert!(!is_included(&path));
    }

    fn make_exclusion(file_pattern: FilePattern, project_root: Option<&Path>) -> GlobSet {
        let mut builder = globset::GlobSetBuilder::new();
        file_pattern.add_to(&mut builder, project_root).unwrap();
        builder.build().unwrap()
    }

    #[test]
    fn exclusions() -> Result<()> {
        let project_root = Path::new("/tmp/");

        let path = Path::new("foo").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("foo".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("bar".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("baz.py".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("foo/bar".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("foo/bar/baz.py".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("foo/bar/*.py".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("baz".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(!is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(project_root))
        ));

        Ok(())
    }
}
