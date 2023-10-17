//! Discover Python files, and their corresponding [`Settings`], from the
//! filesystem.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::Result;
use anyhow::{anyhow, bail};
use ignore::{DirEntry, WalkBuilder, WalkState};
use itertools::Itertools;
use log::debug;
use path_absolutize::path_dedot;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_linter::fs;
use ruff_linter::packaging::is_package;

use crate::configuration::Configuration;
use crate::pyproject;
use crate::pyproject::settings_toml;
use crate::settings::Settings;

/// The configuration information from a `pyproject.toml` file.
pub struct PyprojectConfig {
    /// The strategy used to discover the relevant `pyproject.toml` file for
    /// each Python file.
    pub strategy: PyprojectDiscoveryStrategy,
    /// All settings from the `pyproject.toml` file.
    pub settings: Settings,
    /// Absolute path to the `pyproject.toml` file. This would be `None` when
    /// either using the default settings or the `--isolated` flag is set.
    pub path: Option<PathBuf>,
}

impl PyprojectConfig {
    pub fn new(
        strategy: PyprojectDiscoveryStrategy,
        settings: Settings,
        path: Option<PathBuf>,
    ) -> Self {
        Self {
            strategy,
            settings,
            path: path.map(fs::normalize_path),
        }
    }
}

/// The strategy used to discover the relevant `pyproject.toml` file for each
/// Python file.
#[derive(Debug, Copy, Clone)]
pub enum PyprojectDiscoveryStrategy {
    /// Use a fixed `pyproject.toml` file for all Python files (i.e., one
    /// provided on the command-line).
    Fixed,
    /// Use the closest `pyproject.toml` file in the filesystem hierarchy, or
    /// the default settings.
    Hierarchical,
}

impl PyprojectDiscoveryStrategy {
    pub const fn is_fixed(self) -> bool {
        matches!(self, PyprojectDiscoveryStrategy::Fixed)
    }

    pub const fn is_hierarchical(self) -> bool {
        matches!(self, PyprojectDiscoveryStrategy::Hierarchical)
    }
}

/// The strategy for resolving file paths in a `pyproject.toml`.
#[derive(Copy, Clone)]
pub enum Relativity {
    /// Resolve file paths relative to the current working directory.
    Cwd,
    /// Resolve file paths relative to the directory containing the
    /// `pyproject.toml`.
    Parent,
}

impl Relativity {
    pub fn resolve(self, path: &Path) -> PathBuf {
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
    /// Add a resolved [`Settings`] under a given [`PathBuf`] scope.
    fn add(&mut self, path: PathBuf, settings: Settings) {
        self.settings.insert(path, settings);
    }

    /// Return the appropriate [`Settings`] for a given [`Path`].
    pub fn resolve<'a>(
        &'a self,
        path: &Path,
        pyproject_config: &'a PyprojectConfig,
    ) -> &'a Settings {
        match pyproject_config.strategy {
            PyprojectDiscoveryStrategy::Fixed => &pyproject_config.settings,
            PyprojectDiscoveryStrategy::Hierarchical => self
                .settings
                .iter()
                .rev()
                .find_map(|(root, settings)| path.starts_with(root).then_some(settings))
                .unwrap_or(&pyproject_config.settings),
        }
    }

    /// Return a mapping from Python package to its package root.
    pub fn package_roots<'a>(
        &'a self,
        files: &[&'a Path],
        pyproject_config: &'a PyprojectConfig,
    ) -> FxHashMap<&'a Path, Option<&'a Path>> {
        // Pre-populate the module cache, since the list of files could (but isn't
        // required to) contain some `__init__.py` files.
        let mut package_cache: FxHashMap<&Path, bool> = FxHashMap::default();
        for file in files {
            if file.ends_with("__init__.py") {
                if let Some(parent) = file.parent() {
                    package_cache.insert(parent, true);
                }
            }
        }

        // Search for the package root for each file.
        let mut package_roots: FxHashMap<&Path, Option<&Path>> = FxHashMap::default();
        for file in files {
            let namespace_packages = &self
                .resolve(file, pyproject_config)
                .linter
                .namespace_packages;
            if let Some(package) = file.parent() {
                if package_roots.contains_key(package) {
                    continue;
                }
                package_roots.insert(
                    package,
                    detect_package_root_with_cache(package, namespace_packages, &mut package_cache),
                );
            }
        }

        package_roots
    }

    /// Return an iterator over the resolved [`Settings`] in this [`Resolver`].
    pub fn settings(&self) -> impl Iterator<Item = &Settings> {
        self.settings.values()
    }
}

/// A wrapper around `detect_package_root` to cache filesystem lookups.
fn detect_package_root_with_cache<'a>(
    path: &'a Path,
    namespace_packages: &'a [PathBuf],
    package_cache: &mut FxHashMap<&'a Path, bool>,
) -> Option<&'a Path> {
    let mut current = None;
    for parent in path.ancestors() {
        if !is_package_with_cache(parent, namespace_packages, package_cache) {
            return current;
        }
        current = Some(parent);
    }
    current
}

/// A wrapper around `is_package` to cache filesystem lookups.
fn is_package_with_cache<'a>(
    path: &'a Path,
    namespace_packages: &'a [PathBuf],
    package_cache: &mut FxHashMap<&'a Path, bool>,
) -> bool {
    *package_cache
        .entry(path)
        .or_insert_with(|| is_package(path, namespace_packages))
}

/// Applies a transformation to a [`Configuration`].
///
/// Used to override options with the the values provided by the CLI.
pub trait ConfigurationTransformer: Sync {
    fn transform(&self, config: Configuration) -> Configuration;
}

/// Recursively resolve a [`Configuration`] from a `pyproject.toml` file at the
/// specified [`Path`].
// TODO(charlie): This whole system could do with some caching. Right now, if a
// configuration file extends another in the same path, we'll re-parse the same
// file at least twice (possibly more than twice, since we'll also parse it when
// resolving the "default" configuration).
fn resolve_configuration(
    pyproject: &Path,
    relativity: Relativity,
    transformer: &dyn ConfigurationTransformer,
) -> Result<Configuration> {
    let mut seen = FxHashSet::default();
    let mut stack = vec![];
    let mut next = Some(fs::normalize_path(pyproject));
    while let Some(path) = next {
        if seen.contains(&path) {
            bail!("Circular dependency detected in pyproject.toml");
        }

        // Resolve the current path.
        let options = pyproject::load_options(&path)
            .map_err(|err| anyhow!("Failed to parse `{}`: {}", path.display(), err))?;

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
    Ok(transformer.transform(configuration))
}

/// Extract the project root (scope) and [`Settings`] from a given
/// `pyproject.toml`.
fn resolve_scoped_settings(
    pyproject: &Path,
    relativity: Relativity,
    transformer: &dyn ConfigurationTransformer,
) -> Result<(PathBuf, Settings)> {
    let configuration = resolve_configuration(pyproject, relativity, transformer)?;
    let project_root = relativity.resolve(pyproject);
    let settings = configuration.into_settings(&project_root)?;
    Ok((project_root, settings))
}

/// Extract the [`Settings`] from a given `pyproject.toml` and process the
/// configuration with the given [`ConfigurationTransformer`].
pub fn resolve_root_settings(
    pyproject: &Path,
    relativity: Relativity,
    transformer: &dyn ConfigurationTransformer,
) -> Result<Settings> {
    let (_project_root, settings) = resolve_scoped_settings(pyproject, relativity, transformer)?;
    Ok(settings)
}

/// Find all Python (`.py`, `.pyi` and `.ipynb` files) in a set of paths.
pub fn python_files_in_path(
    paths: &[PathBuf],
    pyproject_config: &PyprojectConfig,
    transformer: &dyn ConfigurationTransformer,
) -> Result<(Vec<Result<DirEntry, ignore::Error>>, Resolver)> {
    // Normalize every path (e.g., convert from relative to absolute).
    let mut paths: Vec<PathBuf> = paths.iter().map(fs::normalize_path).unique().collect();

    // Search for `pyproject.toml` files in all parent directories.
    let mut resolver = Resolver::default();
    let mut seen = FxHashSet::default();
    if pyproject_config.strategy.is_hierarchical() {
        for path in &paths {
            for ancestor in path.ancestors() {
                if seen.insert(ancestor) {
                    if let Some(pyproject) = settings_toml(ancestor)? {
                        let (root, settings) =
                            resolve_scoped_settings(&pyproject, Relativity::Parent, transformer)?;
                        resolver.add(root, settings);
                    }
                }
            }
        }
    }

    // Check if the paths themselves are excluded.
    if pyproject_config.settings.file_resolver.force_exclude {
        paths.retain(|path| !is_file_excluded(path, &resolver, pyproject_config));
        if paths.is_empty() {
            return Ok((vec![], resolver));
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
    builder.standard_filters(pyproject_config.settings.file_resolver.respect_gitignore);
    builder.hidden(false);
    let walker = builder.build_parallel();

    // Run the `WalkParallel` to collect all Python files.
    let error: std::sync::Mutex<Result<()>> = std::sync::Mutex::new(Ok(()));
    let resolver: RwLock<Resolver> = RwLock::new(resolver);
    let files: std::sync::Mutex<Vec<Result<DirEntry, ignore::Error>>> =
        std::sync::Mutex::new(vec![]);
    walker.run(|| {
        Box::new(|result| {
            // Respect our own exclusion behavior.
            if let Ok(entry) = &result {
                if entry.depth() > 0 {
                    let path = entry.path();
                    let resolver = resolver.read().unwrap();
                    let settings = resolver.resolve(path, pyproject_config);
                    if let Some(file_name) = path.file_name() {
                        if !settings.file_resolver.exclude.is_empty()
                            && match_exclusion(path, file_name, &settings.file_resolver.exclude)
                        {
                            debug!("Ignored path via `exclude`: {:?}", path);
                            return WalkState::Skip;
                        } else if !settings.file_resolver.extend_exclude.is_empty()
                            && match_exclusion(
                                path,
                                file_name,
                                &settings.file_resolver.extend_exclude,
                            )
                        {
                            debug!("Ignored path via `extend-exclude`: {:?}", path);
                            return WalkState::Skip;
                        }
                    } else {
                        debug!("Ignored path due to error in parsing: {:?}", path);
                        return WalkState::Skip;
                    }
                }
            }

            // Search for the `pyproject.toml` file in this directory, before we visit any
            // of its contents.
            if pyproject_config.strategy.is_hierarchical() {
                if let Ok(entry) = &result {
                    if entry
                        .file_type()
                        .is_some_and(|file_type| file_type.is_dir())
                    {
                        match settings_toml(entry.path()) {
                            Ok(Some(pyproject)) => match resolve_scoped_settings(
                                &pyproject,
                                Relativity::Parent,
                                transformer,
                            ) {
                                Ok((root, settings)) => {
                                    resolver.write().unwrap().add(root, settings);
                                }
                                Err(err) => {
                                    *error.lock().unwrap() = Err(err);
                                    return WalkState::Quit;
                                }
                            },
                            Ok(None) => {}
                            Err(err) => {
                                *error.lock().unwrap() = Err(err);
                                return WalkState::Quit;
                            }
                        }
                    }
                }
            }

            if result.as_ref().map_or(true, |entry| {
                // Ignore directories
                if entry.file_type().map_or(true, |ft| ft.is_dir()) {
                    false
                } else if entry.depth() == 0 {
                    // Accept all files that are passed-in directly.
                    true
                } else {
                    // Otherwise, check if the file is included.
                    let path = entry.path();
                    let resolver = resolver.read().unwrap();
                    let settings = resolver.resolve(path, pyproject_config);
                    if settings.file_resolver.include.is_match(path) {
                        debug!("Included path via `include`: {:?}", path);
                        true
                    } else if settings.file_resolver.extend_include.is_match(path) {
                        debug!("Included path via `extend-include`: {:?}", path);
                        true
                    } else {
                        false
                    }
                }
            }) {
                files.lock().unwrap().push(result);
            }

            WalkState::Continue
        })
    });

    error.into_inner().unwrap()?;

    Ok((files.into_inner().unwrap(), resolver.into_inner().unwrap()))
}

/// Return `true` if the Python file at [`Path`] is _not_ excluded.
pub fn python_file_at_path(
    path: &Path,
    pyproject_config: &PyprojectConfig,
    transformer: &dyn ConfigurationTransformer,
) -> Result<bool> {
    if !pyproject_config.settings.file_resolver.force_exclude {
        return Ok(true);
    }

    // Normalize the path (e.g., convert from relative to absolute).
    let path = fs::normalize_path(path);

    // Search for `pyproject.toml` files in all parent directories.
    let mut resolver = Resolver::default();
    if pyproject_config.strategy.is_hierarchical() {
        for ancestor in path.ancestors() {
            if let Some(pyproject) = settings_toml(ancestor)? {
                let (root, settings) =
                    resolve_scoped_settings(&pyproject, Relativity::Parent, transformer)?;
                resolver.add(root, settings);
            }
        }
    }

    // Check exclusions.
    Ok(!is_file_excluded(&path, &resolver, pyproject_config))
}

/// Return `true` if the given top-level [`Path`] should be excluded.
fn is_file_excluded(
    path: &Path,
    resolver: &Resolver,
    pyproject_strategy: &PyprojectConfig,
) -> bool {
    // TODO(charlie): Respect gitignore.
    for path in path.ancestors() {
        if path.file_name().is_none() {
            break;
        }
        let settings = resolver.resolve(path, pyproject_strategy);
        if let Some(file_name) = path.file_name() {
            if !settings.file_resolver.exclude.is_empty()
                && match_exclusion(path, file_name, &settings.file_resolver.exclude)
            {
                debug!("Ignored path via `exclude`: {:?}", path);
                return true;
            } else if !settings.file_resolver.extend_exclude.is_empty()
                && match_exclusion(path, file_name, &settings.file_resolver.extend_exclude)
            {
                debug!("Ignored path via `extend-exclude`: {:?}", path);
                return true;
            }
        } else {
            debug!("Ignored path due to error in parsing: {:?}", path);
            return true;
        }
        if path == settings.file_resolver.project_root {
            // Bail out; we'd end up past the project root on the next iteration
            // (excludes etc. are thus "rooted" to the project).
            break;
        }
    }
    false
}

/// Return `true` if the given file should be ignored based on the exclusion
/// criteria.
fn match_exclusion<P: AsRef<Path>, R: AsRef<Path>>(
    file_path: P,
    file_basename: R,
    exclusion: &globset::GlobSet,
) -> bool {
    exclusion.is_match(file_path) || exclusion.is_match(file_basename)
}

#[cfg(test)]
mod tests {
    use std::fs::{create_dir, File};
    use std::path::Path;

    use anyhow::Result;
    use globset::GlobSet;
    use itertools::Itertools;
    use path_absolutize::Absolutize;
    use tempfile::TempDir;

    use ruff_linter::settings::types::FilePattern;

    use crate::configuration::Configuration;
    use crate::pyproject::find_settings_toml;
    use crate::resolver::{
        is_file_excluded, match_exclusion, python_files_in_path, resolve_root_settings,
        ConfigurationTransformer, PyprojectConfig, PyprojectDiscoveryStrategy, Relativity,
        Resolver,
    };
    use crate::settings::Settings;
    use crate::tests::test_resource_path;

    struct NoOpTransformer;

    impl ConfigurationTransformer for NoOpTransformer {
        fn transform(&self, config: Configuration) -> Configuration {
            config
        }
    }

    #[test]
    fn rooted_exclusion() -> Result<()> {
        let package_root = test_resource_path("package");
        let resolver = Resolver::default();
        let pyproject_config = PyprojectConfig::new(
            PyprojectDiscoveryStrategy::Hierarchical,
            resolve_root_settings(
                &find_settings_toml(&package_root)?.unwrap(),
                Relativity::Parent,
                &NoOpTransformer,
            )?,
            None,
        );
        // src/app.py should not be excluded even if it lives in a hierarchy that should
        // be excluded by virtue of the pyproject.toml having `resources/*` in
        // it.
        assert!(!is_file_excluded(
            &package_root.join("src/app.py"),
            &resolver,
            &pyproject_config,
        ));
        // However, resources/ignored.py should be ignored, since that `resources` is
        // beneath the package root.
        assert!(is_file_excluded(
            &package_root.join("resources/ignored.py"),
            &resolver,
            &pyproject_config,
        ));
        Ok(())
    }

    #[test]
    fn find_python_files() -> Result<()> {
        // Initialize the filesystem:
        //   root
        //   ├── file1.py
        //   ├── dir1.py
        //   │   └── file2.py
        //   └── dir2.py
        let tmp_dir = TempDir::new()?;
        let root = tmp_dir.path();
        let file1 = root.join("file1.py");
        let dir1 = root.join("dir1.py");
        let file2 = dir1.join("file2.py");
        let dir2 = root.join("dir2.py");
        File::create(&file1)?;
        create_dir(dir1)?;
        File::create(&file2)?;
        create_dir(dir2)?;

        let (paths, _) = python_files_in_path(
            &[root.to_path_buf()],
            &PyprojectConfig::new(PyprojectDiscoveryStrategy::Fixed, Settings::default(), None),
            &NoOpTransformer,
        )?;
        let paths = paths
            .iter()
            .flatten()
            .map(ignore::DirEntry::path)
            .sorted()
            .collect::<Vec<_>>();
        assert_eq!(paths, &[file2, file1]);

        Ok(())
    }

    fn make_exclusion(file_pattern: FilePattern) -> GlobSet {
        let mut builder = globset::GlobSetBuilder::new();
        file_pattern.add_to(&mut builder).unwrap();
        builder.build().unwrap()
    }

    #[test]
    fn exclusions() {
        let project_root = Path::new("/tmp/");

        let path = Path::new("foo").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User(
            "foo".to_string(),
            Path::new("foo")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User(
            "bar".to_string(),
            Path::new("bar")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
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
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root).unwrap();
        let exclude = FilePattern::User(
            "foo/bar".to_string(),
            Path::new("foo/bar")
                .absolutize_from(project_root)
                .unwrap()
                .to_path_buf(),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
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
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
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
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
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
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(!match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));
    }
}
