//! Discover Python files, and their corresponding `Settings`, from the
//! filesystem.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use log::debug;
use path_absolutize::path_dedot;
use rustc_hash::FxHashSet;
use walkdir::{DirEntry, WalkDir};

use crate::cli::Overrides;
use crate::fs;
use crate::settings::configuration::Configuration;
use crate::settings::{pyproject, Settings};

/// The strategy for discovering a `pyproject.toml` file for each Python file.
pub enum Strategy {
    /// Use a fixed `pyproject.toml` file for all Python files (i.e., one
    /// provided on the command-line).
    Fixed,
    /// Use the closest `pyproject.toml` file in the filesystem hierarchy, or
    /// the default settings.
    Hierarchical,
}

/// The strategy for resolving file paths in a `pyproject.toml`.
pub enum Relativity {
    /// Resolve file paths relative to the current working directory.
    Cwd,
    /// Resolve file paths relative to the directory containing the
    /// `pyproject.toml`.
    Parent,
}

impl Relativity {
    pub fn resolve(&self, path: &Path) -> PathBuf {
        match self {
            Relativity::Parent => path.parent().unwrap().to_path_buf(),
            Relativity::Cwd => path_dedot::CWD.clone(),
        }
    }
}

#[derive(Default)]
pub struct Resolver {
    settings: BTreeMap<PathBuf, Settings>,
}

impl Resolver {
    /// Merge a `Resolver` into the current `Resolver`.
    pub fn merge(&mut self, resolver: Resolver) {
        self.settings.extend(resolver.settings);
    }

    /// Add a resolved `Settings` under a given `PathBuf` scope.
    pub fn add(&mut self, path: PathBuf, settings: Settings) {
        self.settings.insert(path, settings);
    }

    /// Return the appropriate `Settings` for a given `Path`.
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

/// Recursively resolve a `Configuration` from a `pyproject.toml` file at the
/// specified `Path`.
pub fn resolve_configuration(
    pyproject: &Path,
    relativity: &Relativity,
    overrides: Option<&Overrides>,
) -> Result<Configuration> {
    let mut seen = FxHashSet::default();
    let mut stack = vec![];
    let mut next = Some(fs::normalize_path(pyproject));
    while let Some(path) = next {
        if seen.contains(&path) {
            bail!("Circular dependency detected in pyproject.toml");
        }

        // Resolve the current path.
        let options = pyproject::load_options(&path)?;
        let project_root = relativity.resolve(&path);
        let configuration = Configuration::from_options(options, &project_root)?;

        // If extending, continue to collect.
        next = configuration
            .extend
            .as_ref()
            .map(|extend| fs::normalize_path_to(extend, &project_root));

        // Keep track of (1) the paths we've already resolved (to avoid cycles), and (2)
        // the base configuration for every path.
        seen.insert(path);
        stack.push(configuration);
    }

    // Merge the configurations, in order.
    stack.reverse();
    let mut configuration = stack
        .pop()
        .expect("Expected to have at least one Configuration");
    while let Some(extend) = stack.pop() {
        configuration = configuration.combine(extend);
    }
    if let Some(overrides) = overrides {
        configuration.apply(overrides.clone());
    }
    Ok(configuration)
}

/// Extract the project root (scope) and `Settings` from a given
/// `pyproject.toml`.
pub fn resolve_scoped_settings(
    pyproject: &Path,
    relativity: &Relativity,
    overrides: Option<&Overrides>,
) -> Result<(PathBuf, Settings)> {
    let project_root = relativity.resolve(pyproject);
    let configuration = resolve_configuration(pyproject, relativity, overrides)?;
    let settings = Settings::from_configuration(configuration, &project_root)?;
    Ok((project_root, settings))
}

/// Extract the `Settings` from a given `pyproject.toml`.
pub fn resolve_settings(
    pyproject: &Path,
    relativity: &Relativity,
    overrides: Option<&Overrides>,
) -> Result<Settings> {
    let (_project_root, settings) = resolve_scoped_settings(pyproject, relativity, overrides)?;
    Ok(settings)
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
) -> Result<(Vec<Result<DirEntry, walkdir::Error>>, Resolver)> {
    let mut files = Vec::new();
    let mut resolver = Resolver::default();
    for path in paths {
        let (files_in_path, file_resolver) =
            python_files_in_path(path, strategy, overrides, default)?;
        files.extend(files_in_path);
        resolver.merge(file_resolver);
    }
    Ok((files, resolver))
}

/// Find all Python (`.py` and `.pyi` files) in a given `Path`.
fn python_files_in_path<'a>(
    path: &'a Path,
    strategy: &Strategy,
    overrides: &'a Overrides,
    default: &'a Settings,
) -> Result<(Vec<Result<DirEntry, walkdir::Error>>, Resolver)> {
    let path = fs::normalize_path(path);

    // Search for `pyproject.toml` files in all parent directories.
    let mut resolver = Resolver::default();
    for path in path.ancestors() {
        if path.is_dir() {
            let pyproject = path.join("pyproject.toml");
            if pyproject.is_file() {
                let (root, settings) =
                    resolve_scoped_settings(&pyproject, &Relativity::Parent, Some(overrides))?;
                resolver.add(root, settings);
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
                    // TODO(charlie): Return a `Result` here.
                    let (root, settings) =
                        resolve_scoped_settings(&pyproject, &Relativity::Parent, Some(overrides))
                            .unwrap();
                    resolver.add(root, settings);
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

    Ok((files, resolver))
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

    fn make_exclusion(file_pattern: FilePattern) -> GlobSet {
        let mut builder = globset::GlobSetBuilder::new();
        file_pattern.add_to(&mut builder).unwrap();
        builder.build().unwrap()
    }

    #[test]
    fn exclusions() -> Result<()> {
        let project_root = Path::new("/tmp/");

        let path = Path::new("foo").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User(
            "foo".to_string(),
            Path::new("foo")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User(
            "bar".to_string(),
            Path::new("bar")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User(
            "baz.py".to_string(),
            Path::new("baz.py")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User(
            "foo/bar".to_string(),
            Path::new("foo/bar")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User(
            "foo/bar/baz.py".to_string(),
            Path::new("foo/bar/baz.py")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User(
            "foo/bar/*.py".to_string(),
            Path::new("foo/bar/*.py")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        let path = Path::new("foo/bar/baz.py")
            .absolutize_from(project_root)
            .unwrap();
        let exclude = FilePattern::User(
            "baz".to_string(),
            Path::new("baz")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let (file_path, file_basename) = fs::extract_path_names(&path)?;
        assert!(!is_excluded(
            file_path,
            file_basename,
            &make_exclusion(exclude,)
        ));

        Ok(())
    }
}
