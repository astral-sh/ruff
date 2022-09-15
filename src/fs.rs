use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use anyhow::Result;
use glob::Pattern;
use log::debug;
use walkdir::{DirEntry, WalkDir};

use crate::gitignore;

fn is_excluded(path: &Path, exclude: &[Pattern]) -> bool {
    if let Some(file_name) = path.file_name() {
        if let Some(file_name) = file_name.to_str() {
            for pattern in exclude {
                if pattern.matches(file_name) {
                    return true;
                }
            }
            false
        } else {
            false
        }
    } else {
        false
    }
}

fn is_included(path: &Path) -> bool {
    let file_name = path.to_string_lossy();
    file_name.ends_with(".py") || file_name.ends_with(".pyi")
}

pub fn iter_python_files<'a>(
    path: &'a PathBuf,
    exclude: &'a [Pattern],
    extend_exclude: &'a [Pattern],
    gitignore: &'a Option<gitignore::File<'a>>,
) -> Vec<DirEntry> {
    let skip_filter = exclude.is_empty()
        && extend_exclude.is_empty()
        && gitignore
            .as_ref()
            .map(|file| file.is_empty())
            .unwrap_or(true);
    WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_entry(|entry| {
            if skip_filter {
                return true;
            }

            let path = entry.path();
            if is_excluded(path, exclude) {
                debug!("Ignored path via `exclude`: {:?}", path);
                false
            } else if is_excluded(path, extend_exclude) {
                debug!("Ignored path via `extend-exclude`: {:?}", path);
                false
            } else if gitignore
                .as_ref()
                .and_then(|file| file.is_excluded(path).ok())
                .unwrap_or_default()
            {
                debug!("Ignored path via `.gitignore`: {:?}", path);
                true
            } else {
                true
            }
        })
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            is_included(path)
        })
        .collect()
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

    use glob::Pattern;

    use crate::fs::{is_excluded, is_included};

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz.py");
        assert!(is_included(path));

        let path = Path::new("foo/bar/baz.pyi");
        assert!(is_included(path));

        let path = Path::new("foo/bar/baz.js");
        assert!(!is_included(path));

        let path = Path::new("foo/bar/baz");
        assert!(!is_included(path));
    }

    #[test]
    fn exclusions() {
        let path = Path::new("foo");
        let exclude = vec![Pattern::new("foo").unwrap()];
        assert!(is_excluded(path, &exclude));

        let path = Path::new("foo/bar");
        let exclude = vec![Pattern::new("bar").unwrap()];
        assert!(is_excluded(path, &exclude));

        let path = Path::new("foo/bar/baz.py");
        let exclude = vec![Pattern::new("baz.py").unwrap()];
        assert!(is_excluded(path, &exclude));

        let path = Path::new("foo/bar/baz.py");
        let exclude = vec![Pattern::new("baz").unwrap()];
        assert!(!is_excluded(path, &exclude));
    }
}
