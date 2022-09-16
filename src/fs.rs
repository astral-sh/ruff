use std::borrow::Cow;
use std::env;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::Result;
use log::debug;
use path_absolutize::Absolutize;
use walkdir::{DirEntry, WalkDir};

use crate::settings::FilePattern;

fn is_excluded(path: &Path, exclude: &[FilePattern]) -> bool {
    // Check the basename.
    if let Some(file_name) = path.file_name() {
        if let Some(file_name) = file_name.to_str() {
            for pattern in exclude {
                if pattern.basename.matches(file_name) {
                    return true;
                }
            }
        }
    }

    // Check the complete path.
    if let Some(file_name) = path.to_str() {
        for pattern in exclude {
            if pattern
                .absolute
                .as_ref()
                .map(|pattern| pattern.matches(file_name))
                .unwrap_or_default()
            {
                return true;
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
    WalkDir::new(normalize_path(path))
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            if exclude.is_empty() && extend_exclude.is_empty() {
                return true;
            }

            let path = entry.path();
            if is_excluded(path, exclude) {
                debug!("Ignored path via `exclude`: {:?}", path);
                false
            } else if is_excluded(path, extend_exclude) {
                debug!("Ignored path via `extend-exclude`: {:?}", path);
                false
            } else {
                true
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
    if let Ok(root) = env::current_dir() {
        if let Ok(path) = path.strip_prefix(root) {
            return path.to_string_lossy();
        }
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

    use path_absolutize::Absolutize;

    use crate::fs::{is_excluded, is_included};
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
    fn exclusions() {
        let path = Path::new("foo").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("foo")];
        assert!(is_excluded(&path, &exclude));

        let path = Path::new("foo/bar").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("bar")];
        assert!(is_excluded(&path, &exclude));

        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("baz.py")];
        assert!(is_excluded(&path, &exclude));

        let path = Path::new("foo/bar").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("foo/bar")];
        assert!(is_excluded(&path, &exclude));

        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("foo/bar/baz.py")];
        assert!(is_excluded(&path, &exclude));

        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("foo/bar/*.py")];
        assert!(is_excluded(&path, &exclude));

        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        let exclude = vec![FilePattern::user_provided("baz")];
        assert!(!is_excluded(&path, &exclude));
    }
}
