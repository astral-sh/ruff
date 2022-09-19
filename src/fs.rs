use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Read};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Result;
use log::debug;
use path_absolutize::path_dedot;
use path_absolutize::Absolutize;
use walkdir::{DirEntry, WalkDir};

use crate::settings::FilePattern;

fn is_excluded(path: &Path, exclude: &[FilePattern]) -> bool {
    if let Some(file_absolute_name) = path.to_str() {
        if let Some(file_name) = path.file_name() {
            if let Some(file_basename) = file_name.to_str() {
                for pattern in exclude {
                    match pattern {
                        FilePattern::Simple(basename) => {
                            if *basename == file_basename {
                                return true;
                            }
                        }
                        FilePattern::Complex(basename, basename_glob, absolute, absolute_glob) => {
                            // Check the basename, as a simple path and a glob pattern.
                            if let Some(basename) = basename {
                                if basename == file_basename {
                                    return true;
                                }
                            }
                            if let Some(basename_glob) = basename_glob {
                                if basename_glob.matches(file_basename) {
                                    return true;
                                }
                            }
                            // Check the absolute name, as a simple path and a glob pattern.
                            if let Some(absolute) = absolute {
                                if absolute == file_absolute_name {
                                    return true;
                                }
                            }
                            if let Some(absolute_glob) = absolute_glob {
                                if absolute_glob.matches(file_absolute_name) {
                                    return true;
                                }
                            }
                        }
                    }
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
            let file_type = entry.file_type();

            if has_exclude && (!exclude_simple || file_type.is_dir()) && is_excluded(path, exclude)
            {
                debug!("Ignored path via `exclude`: {:?}", path);
                false
            } else if has_extend_exclude
                && (!extend_exclude_simple || file_type.is_dir())
                && is_excluded(path, extend_exclude)
            {
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
