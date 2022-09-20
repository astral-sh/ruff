use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Read};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use log::debug;
use path_absolutize::path_dedot;
use path_absolutize::Absolutize;
use walkdir::{DirEntry, WalkDir};

use crate::settings::FilePattern;

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

fn is_excluded(file_path: &str, file_basename: &str, exclude: &[FilePattern]) -> bool {
    for pattern in exclude {
        match &pattern {
            FilePattern::Simple(basename) => {
                if *basename == file_basename {
                    return true;
                }
            }
            FilePattern::Complex(absolute, basename) => {
                if absolute.matches(file_path) {
                    return true;
                }
                if basename
                    .as_ref()
                    .map(|pattern| pattern.matches(file_basename))
                    .unwrap_or_default()
                {
                    return true;
                }
            }
        }
    }
    false
}

fn is_included(path: &Path) -> bool {
    let file_name = path.to_string_lossy();
    file_name.ends_with(".py") || file_name.ends_with(".pyi")
}

pub fn iter_python_files<'a>(
    path: &'a Path,
    exclude: &'a [FilePattern],
    extend_exclude: &'a [FilePattern],
) -> impl Iterator<Item = DirEntry> + 'a {
    // Run some checks over the provided patterns, to enable optimizations below.
    let has_exclude = !exclude.is_empty();
    let has_extend_exclude = !extend_exclude.is_empty();
    let exclude_simple = exclude
        .iter()
        .all(|pattern| matches!(pattern, FilePattern::Simple(_)));
    let extend_exclude_simple = exclude
        .iter()
        .all(|pattern| matches!(pattern, FilePattern::Simple(_)));

    WalkDir::new(normalize_path(path))
        .follow_links(true)
        .into_iter()
        .filter_entry(move |entry| {
            if !has_exclude && !has_extend_exclude {
                return true;
            }

            let path = entry.path();
            match extract_path_names(path) {
                Ok((file_path, file_basename)) => {
                    let file_type = entry.file_type();

                    if has_exclude
                        && (!exclude_simple || file_type.is_dir())
                        && is_excluded(file_path, file_basename, exclude)
                    {
                        debug!("Ignored path via `exclude`: {:?}", path);
                        false
                    } else if has_extend_exclude
                        && (!extend_exclude_simple || file_type.is_dir())
                        && is_excluded(file_path, file_basename, extend_exclude)
                    {
                        debug!("Ignored path via `extend-exclude`: {:?}", path);
                        false
                    } else {
                        true
                    }
                }
                Err(_) => {
                    debug!("Ignored path due to error in parsing: {:?}", path);
                    true
                }
            }
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            is_included(path)
        })
}

pub fn normalize_path(path: &Path) -> PathBuf {
    if path == Path::new(".") || path == Path::new("..") {
        return path.to_path_buf();
    }
    if let Ok(path) = path.absolutize() {
        return path.to_path_buf();
    }
    path.to_path_buf()
}

pub fn relativize_path(path: &Path) -> Cow<str> {
    if let Ok(path) = path.strip_prefix(path_dedot::CWD.deref()) {
        return path.to_string_lossy();
    }
    path.to_string_lossy()
}

pub fn read_file(path: &Path) -> Result<String> {
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
    use path_absolutize::Absolutize;

    use crate::fs::{extract_path_names, is_excluded, is_included};
    use crate::settings::FilePattern;

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

    #[test]
    fn exclusions() -> Result<()> {
        let exclude = vec![FilePattern::from_user("foo")];
        let path = Path::new("foo").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(file_path, file_basename, &exclude));

        let exclude = vec![FilePattern::from_user("bar")];
        let path = Path::new("bar").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(file_path, file_basename, &exclude));

        let exclude = vec![FilePattern::from_user("baz.py")];
        let path = Path::new("baz.py").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(file_path, file_basename, &exclude));

        let exclude = vec![FilePattern::from_user("foo/bar")];
        let path = Path::new("foo/bar").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(file_path, file_basename, &exclude));

        let exclude = vec![FilePattern::from_user("foo/bar/baz.py")];
        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(file_path, file_basename, &exclude));

        let exclude = vec![FilePattern::from_user("foo/bar/*.py")];
        let path = Path::new("foo/bar/*.py").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(is_excluded(file_path, file_basename, &exclude));

        let exclude = vec![FilePattern::from_user("baz")];
        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        let (file_path, file_basename) = extract_path_names(&path)?;
        assert!(!is_excluded(file_path, file_basename, &exclude));

        Ok(())
    }
}
