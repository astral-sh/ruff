use std::borrow::Cow;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use globset::GlobMatcher;
use log::debug;
use path_absolutize::{path_dedot, Absolutize};
use walkdir::{DirEntry, WalkDir};

use crate::checks::CheckCode;

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
    let file_name = path.to_string_lossy();
    file_name.ends_with(".py") || file_name.ends_with(".pyi")
}

pub fn iter_python_files<'a>(
    path: &'a Path,
    exclude: &'a globset::GlobSet,
    extend_exclude: &'a globset::GlobSet,
) -> impl Iterator<Item = Result<DirEntry, walkdir::Error>> + 'a {
    // Run some checks over the provided patterns, to enable optimizations below.
    let has_exclude = !exclude.is_empty();
    let has_extend_exclude = !extend_exclude.is_empty();

    WalkDir::new(normalize_path(path))
        .into_iter()
        .filter_entry(move |entry| {
            if !has_exclude && !has_extend_exclude {
                return true;
            }

            let path = entry.path();
            match extract_path_names(path) {
                Ok((file_path, file_basename)) => {
                    if has_exclude && is_excluded(file_path, file_basename, exclude) {
                        debug!("Ignored path via `exclude`: {:?}", path);
                        false
                    } else if has_extend_exclude
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
    pattern_code_pairs: &'a [(GlobMatcher, GlobMatcher, BTreeSet<CheckCode>)],
) -> Result<BTreeSet<&'a CheckCode>> {
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
    use std::path::{Path, PathBuf};

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

    fn make_exclusion(file_pattern: FilePattern, project_root: Option<&PathBuf>) -> GlobSet {
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
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("bar".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("baz.py".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("foo/bar".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("foo/bar/baz.py".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("foo/bar/*.py".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("baz".to_string());
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(!is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, Some(&project_root.to_path_buf()))
        ));

        Ok(())
    }
}
