//! Discover Python files, and their corresponding `Settings`, from the
//! filesystem.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use log::{debug, error};
use walkdir::{DirEntry, WalkDir};

use crate::cli::Overrides;
use crate::fs;
use crate::settings::configuration::Configuration;
use crate::settings::{pyproject, Settings};

pub enum Strategy {
    Fixed,
    Hierarchical,
}

#[derive(Default)]
pub struct Resolver {
    settings: BTreeMap<PathBuf, Settings>,
}

impl Resolver {
    pub fn merge(&mut self, resolver: Resolver) {
        self.settings.extend(resolver.settings);
    }

    pub fn add(&mut self, path: PathBuf, settings: Settings) {
        self.settings.insert(path, settings);
    }

    pub fn resolve(&self, path: &Path, strategy: &Strategy) -> Option<&Settings> {
        match strategy {
            Strategy::Fixed => None,
            Strategy::Hierarchical => self.settings.iter().rev().find_map(|(root, settings)| {
                if path.starts_with(root) {
                    Some(settings)
                } else {
                    None
                }
            }),
        }
    }
}

/// Extract the `Settings` from a given `pyproject.toml`.
pub fn settings_for_path(pyproject: &Path, overrides: &Overrides) -> Result<(PathBuf, Settings)> {
    let project_root = pyproject
        .parent()
        .ok_or_else(|| anyhow!("Expected pyproject.toml to be in a directory"))?
        .to_path_buf();
    let options = pyproject::load_options(pyproject)?;
    let mut configuration = Configuration::from_options(options)?;
    configuration.merge(overrides.clone());
    let settings = Settings::from_configuration(configuration, &project_root)?;
    Ok((project_root, settings))
}

/// Return `true` if the given file should be ignored based on the exclusion
/// criteria.
fn is_excluded(file_path: &str, file_basename: &str, exclude: &globset::GlobSet) -> bool {
    exclude.is_match(file_path) || exclude.is_match(file_basename)
}

/// Return `true` if the `Path` appears to be that of a Python file.
fn is_python_file(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "py" || ext == "pyi")
}

/// Find all Python (`.py` and `.pyi` files) in a set of `Path`.
pub fn resolve_python_files<'a>(
    paths: &'a [PathBuf],
    strategy: &Strategy,
    overrides: &'a Overrides,
    default: &'a Settings,
) -> (Vec<Result<DirEntry, walkdir::Error>>, Resolver) {
    let mut files = Vec::new();
    let mut resolver = Resolver::default();
    for path in paths {
        let (files_in_path, file_resolver) =
            python_files_in_path(path, strategy, overrides, default);
        files.extend(files_in_path);
        resolver.merge(file_resolver);
    }
    (files, resolver)
}

/// Find all Python (`.py` and `.pyi` files) in a given `Path`.
fn python_files_in_path<'a>(
    path: &'a Path,
    strategy: &Strategy,
    overrides: &'a Overrides,
    default: &'a Settings,
) -> (Vec<Result<DirEntry, walkdir::Error>>, Resolver) {
    let path = fs::normalize_path(path);

    // Search for `pyproject.toml` files in all parent directories.
    let mut resolver = Resolver::default();
    for path in path.ancestors() {
        if path.is_dir() {
            let pyproject = path.join("pyproject.toml");
            if pyproject.is_file() {
                match settings_for_path(&pyproject, overrides) {
                    Ok((root, settings)) => resolver.add(root, settings),
                    Err(err) => error!("Failed to read settings: {err}"),
                }
            }
        }
    }

    // Collect all Python files.
    let files: Vec<Result<DirEntry, walkdir::Error>> = WalkDir::new(path)
        .into_iter()
        .filter_entry(|entry| {
            // Search for the `pyproject.toml` file in this directory, before we visit any
            // of its contents.
            if entry.file_type().is_dir() {
                let pyproject = entry.path().join("pyproject.toml");
                if pyproject.is_file() {
                    match settings_for_path(&pyproject, overrides) {
                        Ok((root, settings)) => resolver.add(root, settings),
                        Err(err) => error!("Failed to read settings: {err}"),
                    }
                }
            }

            let path = entry.path();
            let settings = resolver.resolve(path, strategy).unwrap_or(default);
            match fs::extract_path_names(path) {
                Ok((file_path, file_basename)) => {
                    if !settings.exclude.is_empty()
                        && is_excluded(file_path, file_basename, &settings.exclude)
                    {
                        debug!("Ignored path via `exclude`: {:?}", path);
                        false
                    } else if !settings.extend_exclude.is_empty()
                        && is_excluded(file_path, file_basename, &settings.extend_exclude)
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
                (entry.depth() == 0 || is_python_file(entry.path()))
                    && !entry.file_type().is_dir()
                    && !(entry.file_type().is_symlink() && entry.path().is_dir())
            })
        })
        .collect::<Vec<_>>();

    (files, resolver)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use globset::GlobSet;
    use path_absolutize::Absolutize;

    use crate::fs;
    use crate::resolver::{is_excluded, is_python_file};
    use crate::settings::types::FilePattern;

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        assert!(is_python_file(&path));

        let path = Path::new("foo/bar/baz.pyi").absolutize().unwrap();
        assert!(is_python_file(&path));

        let path = Path::new("foo/bar/baz.js").absolutize().unwrap();
        assert!(!is_python_file(&path));

        let path = Path::new("foo/bar/baz").absolutize().unwrap();
        assert!(!is_python_file(&path));
    }

    fn make_exclusion(file_pattern: FilePattern, project_root: &Path) -> GlobSet {
        let mut builder = globset::GlobSetBuilder::new();
        file_pattern.add_to(&mut builder, project_root).unwrap();
        builder.build().unwrap()
    }

    #[test]
    fn exclusions() -> Result<()> {
        let project_root = Path::new("/tmp/");

        let path = Path::new("foo").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("foo".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("bar".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("baz.py".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User("foo/bar".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("foo/bar/baz.py".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("foo/bar/*.py".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User("baz".to_string());
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(!is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude, project_root)
        ));

        Ok(())
    }
}
