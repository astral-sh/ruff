//! Discover Python files, and their corresponding `Settings`, from the
//! filesystem.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::{anyhow, bail, Result};
use ignore::{DirEntry, WalkBuilder, WalkState};
use log::debug;
use path_absolutize::path_dedot;
use rustc_hash::FxHashSet;

use crate::cli::Overrides;
use crate::fs;
use crate::settings::configuration::Configuration;
use crate::settings::pyproject::has_ruff_section;
use crate::settings::{pyproject, Settings};

/// The strategy used to discover Python files in the filesystem..
#[derive(Debug)]
pub struct FileDiscovery {
    pub force_exclude: bool,
    pub respect_gitignore: bool,
}

/// The strategy used to discover the relevant `pyproject.toml` file for each
/// Python file.
#[derive(Debug)]
pub enum PyprojectDiscovery {
    /// Use a fixed `pyproject.toml` file for all Python files (i.e., one
    /// provided on the command-line).
    Fixed(Settings),
    /// Use the closest `pyproject.toml` file in the filesystem hierarchy, or
    /// the default settings.
    Hierarchical(Settings),
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
            Relativity::Parent => path
                .parent()
                .expect("Expected pyproject.toml file to be in parent directory")
                .to_path_buf(),
            Relativity::Cwd => path_dedot::CWD.clone(),
        }
    }
}

#[derive(Default)]
pub struct Resolver {
    settings: BTreeMap<PathBuf, Settings>,
}

impl Resolver {
    /// Add a resolved `Settings` under a given `PathBuf` scope.
    pub fn add(&mut self, path: PathBuf, settings: Settings) {
        self.settings.insert(path, settings);
    }

    /// Return the appropriate `Settings` for a given `Path`.
    pub fn resolve<'a>(&'a self, path: &Path, strategy: &'a PyprojectDiscovery) -> &'a Settings {
        match strategy {
            PyprojectDiscovery::Fixed(settings) => settings,
            PyprojectDiscovery::Hierarchical(default) => self
                .settings
                .iter()
                .rev()
                .find_map(|(root, settings)| {
                    if path.starts_with(root) {
                        Some(settings)
                    } else {
                        None
                    }
                })
                .unwrap_or(default),
        }
    }
}

/// Recursively resolve a `Configuration` from a `pyproject.toml` file at the
/// specified `Path`.
// TODO(charlie): This whole system could do with some caching. Right now, if a
// configuration file extends another in the same path, we'll re-parse the same
// file at least twice (possibly more than twice, since we'll also parse it when
// resolving the "default" configuration).
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
        next = configuration.extend.as_ref().map(|extend| {
            fs::normalize_path_to(
                extend,
                path.parent()
                    .expect("Expected pyproject.toml file to be in parent directory"),
            )
        });

        // Keep track of (1) the paths we've already resolved (to avoid cycles), and (2)
        // the base configuration for every path.
        seen.insert(path);
        stack.push(configuration);
    }

    // Merge the configurations, in order.
    stack.reverse();
    let mut configuration = stack.pop().unwrap();
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
fn is_python_path(path: &Path) -> bool {
    path.extension()
        .map_or(false, |ext| ext == "py" || ext == "pyi")
}

/// Return `true` if the `Entry` appears to be that of a Python file.
pub fn is_python_entry(entry: &DirEntry) -> bool {
    is_python_path(entry.path())
        && !entry
            .file_type()
            .map_or(false, |file_type| file_type.is_dir())
}

/// Find all Python (`.py` and `.pyi` files) in a set of paths.
pub fn python_files_in_path(
    paths: &[PathBuf],
    pyproject_strategy: &PyprojectDiscovery,
    file_strategy: &FileDiscovery,
    overrides: &Overrides,
) -> Result<(Vec<Result<DirEntry, ignore::Error>>, Resolver)> {
    // Normalize every path (e.g., convert from relative to absolute).
    let paths: Vec<PathBuf> = paths.iter().map(fs::normalize_path).collect();

    // Search for `pyproject.toml` files in all parent directories.
    let mut resolver = Resolver::default();
    for path in &paths {
        for ancestor in path.ancestors() {
            let pyproject = ancestor.join("pyproject.toml");
            if pyproject.is_file() {
                if has_ruff_section(&pyproject)? {
                    let (root, settings) =
                        resolve_scoped_settings(&pyproject, &Relativity::Parent, Some(overrides))?;
                    resolver.add(root, settings);
                }
            }
        }
    }

    // Create the `WalkBuilder`.
    let mut builder = WalkBuilder::new(
        paths
            .get(0)
            .ok_or_else(|| anyhow!("Expected at least one path to search for Python files"))?,
    );
    for path in &paths[1..] {
        builder.add(path);
    }
    builder.standard_filters(file_strategy.respect_gitignore);
    builder.hidden(false);
    let walker = builder.build_parallel();

    // Run the `WalkParallel` to collect all Python files.
    let error: std::sync::Mutex<Result<()>> = std::sync::Mutex::new(Ok(()));
    let resolver: RwLock<Resolver> = RwLock::new(resolver);
    let files: std::sync::Mutex<Vec<Result<DirEntry, ignore::Error>>> =
        std::sync::Mutex::new(vec![]);
    walker.run(|| {
        Box::new(|result| {
            // Search for the `pyproject.toml` file in this directory, before we visit any
            // of its contents.
            if let Ok(entry) = &result {
                if entry
                    .file_type()
                    .map_or(false, |file_type| file_type.is_dir())
                {
                    let pyproject = entry.path().join("pyproject.toml");
                    if pyproject.is_file() {
                        match has_ruff_section(&pyproject) {
                            Ok(false) => {}
                            Ok(true) => {
                                match resolve_scoped_settings(
                                    &pyproject,
                                    &Relativity::Parent,
                                    Some(overrides),
                                ) {
                                    Ok((root, settings)) => {
                                        resolver.write().unwrap().add(root, settings);
                                    }
                                    Err(err) => {
                                        *error.lock().unwrap() = Err(err);
                                        return WalkState::Quit;
                                    }
                                }
                            }
                            Err(err) => {
                                *error.lock().unwrap() = Err(err);
                                return WalkState::Quit;
                            }
                        }
                    }
                }
            }

            // Respect our own exclusion behavior.
            if let Ok(entry) = &result {
                if file_strategy.force_exclude || entry.depth() > 0 {
                    let path = entry.path();
                    let resolver = resolver.read().unwrap();
                    let settings = resolver.resolve(path, pyproject_strategy);
                    match fs::extract_path_names(path) {
                        Ok((file_path, file_basename)) => {
                            if !settings.exclude.is_empty()
                                && is_excluded(file_path, file_basename, &settings.exclude)
                            {
                                debug!("Ignored path via `exclude`: {:?}", path);
                                return WalkState::Skip;
                            } else if !settings.extend_exclude.is_empty()
                                && is_excluded(file_path, file_basename, &settings.extend_exclude)
                            {
                                debug!("Ignored path via `extend-exclude`: {:?}", path);
                                return WalkState::Skip;
                            }
                        }
                        Err(err) => {
                            debug!("Ignored path due to error in parsing: {:?}: {}", path, err);
                            return WalkState::Skip;
                        }
                    }
                }
            }

            if result.as_ref().map_or(true, is_python_entry) {
                files.lock().unwrap().push(result);
            }

            WalkState::Continue
        })
    });

    error.into_inner().unwrap()?;

    Ok((files.into_inner().unwrap(), resolver.into_inner().unwrap()))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::Result;
    use globset::GlobSet;
    use path_absolutize::Absolutize;

    use crate::fs;
    use crate::resolver::{is_excluded, is_python_path};
    use crate::settings::types::FilePattern;

    #[test]
    fn inclusions() {
        let path = Path::new("foo/bar/baz.py").absolutize().unwrap();
        assert!(is_python_path(&path));

        let path = Path::new("foo/bar/baz.pyi").absolutize().unwrap();
        assert!(is_python_path(&path));

        let path = Path::new("foo/bar/baz.js").absolutize().unwrap();
        assert!(!is_python_path(&path));

        let path = Path::new("foo/bar/baz").absolutize().unwrap();
        assert!(!is_python_path(&path));
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
