//! Discover Python files, and their corresponding [`Settings`], from the
//! filesystem.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use anyhow::{anyhow, bail};
use globset::{Candidate, GlobSet};
use ignore::{DirEntry, Error, ParallelVisitor, WalkBuilder, WalkState};
use itertools::Itertools;
use log::debug;
use matchit::{InsertError, Match, Router};
use path_absolutize::path_dedot;
use path_slash::PathExt;
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_linter::fs;
use ruff_linter::package::PackageRoot;
use ruff_linter::packaging::{detect_package_root, is_package};
use ruff_linter::settings::LinterSettings;
use ruff_python_ast::SourceType;

use crate::configuration::Configuration;
use crate::pyproject::settings_toml;
use crate::settings::Settings;
use crate::{FileResolverSettings, FormatterSettings, pyproject};

/// The root configuration for a Ruff invocation and how Ruff selects settings for each input file.
///
/// For example, running `ruff check .` in a directory with a `ruff.toml` starts
/// with that file's settings. If `tests/ruff.toml` also exists, its settings
/// apply to files under `tests/`. Running `ruff check --config ruff.toml .`
/// instead uses the specified configuration for every file.
#[derive(Debug)]
pub struct PyprojectDiscovery {
    /// Whether to look for a closer configuration file for each input.
    pub strategy: PyprojectDiscoveryStrategy,
    /// The configuration selected for this invocation (from a file or Ruff's defaults)
    /// and the settings built from it.
    pub root: ConfigurationAndSettings,
}

/// How the resolver chooses a configuration for each input file.
#[derive(Debug, Copy, Clone)]
pub enum PyprojectDiscoveryStrategy {
    /// Use the root settings for every file, as with `--config <path>` or `--isolated`.
    Fixed,
    /// Use the closest configuration in the file's directory or its ancestors,
    /// falling back to the root settings.
    Hierarchical,
}

impl PyprojectDiscoveryStrategy {
    #[inline]
    const fn is_hierarchical(self) -> bool {
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
    fn resolve(self, path: &Path) -> &Path {
        match self {
            Relativity::Parent => path
                .parent()
                .expect("Expected pyproject.toml file to be in parent directory"),
            Relativity::Cwd => &path_dedot::CWD,
        }
    }
}

#[derive(Debug)]
pub struct Resolver<'a> {
    project_config: &'a PyprojectDiscovery,
    /// All [`Settings`] that have been added to the resolver, along with their config file paths.
    settings: Vec<ConfigurationAndSettings>,
    /// A router from path to index into the `settings` vector.
    router: Router<usize>,
}

impl<'a> Resolver<'a> {
    /// Create a new [`Resolver`] for the given [`PyprojectDiscovery`].
    pub fn new(project_config: &'a PyprojectDiscovery) -> Self {
        Self {
            project_config,
            settings: Vec::new(),
            router: Router::new(),
        }
    }

    /// Return the settings from the root configuration.
    #[inline]
    pub fn root_settings(&self) -> &Settings {
        &self.project_config.root.settings
    }

    /// Return `true` if the [`Resolver`] is using a hierarchical discovery strategy.
    #[inline]
    fn is_hierarchical(&self) -> bool {
        self.project_config.strategy.is_hierarchical()
    }

    /// Return `true` if the [`Resolver`] should force-exclude files passed directly to the CLI.
    #[inline]
    pub fn force_exclude(&self) -> bool {
        self.root_settings().file_resolver.force_exclude
    }

    /// Return `true` if the [`Resolver`] should respect `.gitignore` files.
    #[inline]
    fn respect_gitignore(&self) -> bool {
        self.root_settings().file_resolver.respect_gitignore
    }

    /// Use these settings for files under the given directory.
    fn add(&mut self, path: &Path, settings: ConfigurationAndSettings) {
        self.settings.push(settings);

        // Normalize the path to use `/` separators and escape the '{' and '}' characters,
        // which matchit uses for routing parameters.
        let path = path.to_slash_lossy().replace('{', "{{").replace('}', "}}");

        match self
            .router
            .insert(format!("{path}/{{*filepath}}"), self.settings.len() - 1)
        {
            Ok(()) => {}
            Err(InsertError::Conflict { .. }) => {
                return;
            }
            Err(_) => unreachable!("file paths are escaped before being inserted in the router"),
        }

        // Insert a mapping that matches the directory itself (without a trailing slash).
        // Inserting should always succeed because conflicts are resolved above and the above insertion guarantees
        // that the path is correctly escaped.
        self.router.insert(path, self.settings.len() - 1).unwrap();
    }

    /// Return the appropriate [`Settings`] for a given [`Path`].
    pub fn resolve(&self, path: &Path) -> &Settings {
        &self.resolve_with_path(path).settings
    }

    /// Return the settings and their source configuration for a given [`Path`].
    pub fn resolve_with_path(&self, path: &Path) -> &ConfigurationAndSettings {
        match self.project_config.strategy {
            PyprojectDiscoveryStrategy::Fixed => &self.project_config.root,
            PyprojectDiscoveryStrategy::Hierarchical => self
                .router
                .at(path.to_slash_lossy().as_ref())
                .map(|Match { value, .. }| &self.settings[*value])
                .unwrap_or(&self.project_config.root),
        }
    }

    /// Return a mapping from Python package to its package root.
    pub fn package_roots(
        &'a self,
        files: &[&'a Path],
    ) -> FxHashMap<&'a Path, Option<PackageRoot<'a>>> {
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

        // Determine whether any of the settings require namespace packages. If not, we can save
        // a lookup for every file.
        let has_namespace_packages = self
            .settings()
            .any(|settings| !settings.linter.namespace_packages.is_empty());

        // Search for the package root for each file.
        let mut package_roots: FxHashMap<&Path, Option<PackageRoot<'_>>> = FxHashMap::default();
        for file in files {
            if let Some(package) = file.parent() {
                package_roots.entry(package).or_insert_with(|| {
                    let namespace_packages = if has_namespace_packages {
                        self.resolve(file).linter.namespace_packages.as_slice()
                    } else {
                        &[]
                    };
                    detect_package_root_with_cache(package, namespace_packages, &mut package_cache)
                        .map(|path| PackageRoot::Root { path })
                });
            }
        }

        // Discard any nested roots.
        //
        // For example, if `./foo/__init__.py` is a root, and then `./foo/bar` is empty, and
        // `./foo/bar/baz/__init__.py` was detected as a root, we should only consider
        // `./foo/__init__.py`.
        let mut router: Router<&Path> = Router::new();
        for root in package_roots
            .values()
            .flatten()
            .copied()
            .map(PackageRoot::path)
            .collect::<BTreeSet<_>>()
        {
            // Normalize the path to use `/` separators and escape the '{' and '}' characters,
            // which matchit uses for routing parameters.
            let path = root.to_slash_lossy().replace('{', "{{").replace('}', "}}");
            if let Ok(matched) = router.at_mut(&path) {
                debug!(
                    "Ignoring nested package root: {} (under {})",
                    root.display(),
                    matched.value.display()
                );
                package_roots.insert(root, Some(PackageRoot::nested(root)));
            } else {
                let _ = router.insert(format!("{path}/{{*filepath}}"), root);
            }
        }

        package_roots
    }

    /// Return an iterator over the resolved [`Settings`] in this [`Resolver`].
    pub fn settings(&self) -> impl Iterator<Item = &Settings> {
        std::iter::once(self.root_settings())
            .chain(self.settings.iter().map(|entry| &entry.settings))
    }
}

/// A Ruff configuration and the settings built from it.
#[derive(Debug, Default)]
pub struct ConfigurationAndSettings {
    /// The merged configuration, including filesystem fallbacks, before CLI or editor overrides.
    pub configuration: Configuration,
    /// The settings after applying CLI or editor overrides.
    pub settings: Settings,
    /// Absolute path to the Ruff configuration file, if one was loaded.
    pub config_path: Option<PathBuf>,
}

impl ConfigurationAndSettings {
    pub fn new(
        configuration: Configuration,
        transformer: &dyn ConfigurationTransformer,
        project_root: &Path,
        config_path: Option<&Path>,
    ) -> Result<Self> {
        let settings = transformer
            .transform(configuration.clone())
            .into_settings(project_root)?;
        Ok(Self {
            configuration,
            settings,
            config_path: config_path.map(fs::normalize_path),
        })
    }

    pub fn with_overrides<'a, T: ConfigurationTransformer>(
        &'a self,
        overrides: &'a T,
    ) -> SettingsForSource<'a, T> {
        SettingsForSource {
            project: self,
            overrides,
        }
    }
}

/// Project settings with the CLI or editor overrides to apply after reading script metadata.
pub struct SettingsForSource<'a, T> {
    project: &'a ConfigurationAndSettings,
    overrides: &'a T,
}

impl<'a, T: ConfigurationTransformer> SettingsForSource<'a, T> {
    pub fn project_settings(&self) -> &'a Settings {
        &self.project.settings
    }

    /// Resolve settings that depend on the contents of a Python script. An inline
    /// `[tool.ruff]` is a configuration file in its own right, even when empty.
    pub fn resolve_script(
        &self,
        path: Option<&Path>,
        source_type: SourceType,
        source: &str,
    ) -> Result<Option<ConfigurationAndSettings>> {
        if !self
            .project
            .settings
            .should_read_script_metadata(source_type)
        {
            return Ok(None);
        }
        let Some(metadata) =
            pyproject::ScriptMetadata::parse(source, path, self.overrides.use_inline_config())?
        else {
            return Ok(None);
        };

        if let Some(options) = metadata.options {
            let root = fs::normalize_path(path.and_then(Path::parent).unwrap_or(Path::new(".")));
            let mut configuration = Configuration::from_options(options, path, &root)?;
            if let Some(version) = metadata.target_version {
                configuration.target_version.get_or_insert(version.into());
                // Only the script's own per-file targets can override its requirement.
                configuration
                    .per_file_target_version
                    .get_or_insert_default();
            }
            if let Some(extend) = &configuration.extend {
                let base = resolve_configuration(&root.join(extend), ConfigurationOrigin::Unknown)?;
                configuration = configuration.combine(base);
            }
            return ConfigurationAndSettings::new(configuration, self.overrides, &root, path)
                .map(Some);
        }

        let Some(version) = metadata.target_version else {
            return Ok(None);
        };
        let configuration = Configuration {
            target_version: Some(version.into()),
            per_file_target_version: Some(Vec::new()),
            ..self.project.configuration.clone()
        };
        ConfigurationAndSettings::new(
            configuration,
            self.overrides,
            &self.project.settings.file_resolver.project_root,
            self.project.config_path.as_deref(),
        )
        .map(Some)
    }

    pub fn linter(
        &self,
        path: Option<&Path>,
        source_type: SourceType,
        source: &str,
    ) -> Result<Cow<'a, LinterSettings>> {
        Ok(match self.resolve_script(path, source_type, source)? {
            Some(script) => Cow::Owned(script.settings.linter),
            None => Cow::Borrowed(&self.project.settings.linter),
        })
    }

    pub fn formatter(
        &self,
        path: Option<&Path>,
        source_type: SourceType,
        source: &str,
    ) -> Result<Cow<'a, FormatterSettings>> {
        Ok(match self.resolve_script(path, source_type, source)? {
            Some(script) => Cow::Owned(script.settings.formatter),
            None => Cow::Borrowed(&self.project.settings.formatter),
        })
    }

    /// Reuse the package root from file discovery unless the script changes which
    /// directories count as namespace packages.
    pub fn package_root<'p>(
        &self,
        path: &'p Path,
        package: Option<PackageRoot<'p>>,
        linter: &LinterSettings,
    ) -> Option<PackageRoot<'p>> {
        if linter.namespace_packages == self.project.settings.linter.namespace_packages {
            return package;
        }
        let root = detect_package_root(path.parent()?, &linter.namespace_packages)?;
        Some(
            package
                .filter(|package| package.path() == root)
                .unwrap_or_else(|| PackageRoot::root(root)),
        )
    }
}

/// A wrapper around `detect_package_root` to cache filesystem lookups.
fn detect_package_root_with_cache<'a>(
    path: &'a Path,
    namespace_packages: &[PathBuf],
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
    namespace_packages: &[PathBuf],
    package_cache: &mut FxHashMap<&'a Path, bool>,
) -> bool {
    *package_cache
        .entry(path)
        .or_insert_with(|| is_package(path, namespace_packages))
}

/// Applies a transformation to a [`Configuration`].
///
/// Used to override options with the values provided by the CLI.
pub trait ConfigurationTransformer {
    fn transform(&self, config: Configuration) -> Configuration;

    /// Whether to load Ruff configuration embedded in a script, including files it extends.
    fn use_inline_config(&self) -> bool {
        true
    }
}

/// Configurations shared during one traversal, before inheritance and overrides.
///
/// Paths are already normalized, so the project root is part of the cache key.
/// Target-version fallbacks and CLI overrides remain specific to each chain.
#[derive(Default)]
struct ConfigurationCache {
    configurations: RwLock<FxHashMap<(PathBuf, PathBuf), Arc<Configuration>>>,
}

impl ConfigurationCache {
    fn get_or_try_insert_with(
        &self,
        path: &Path,
        project_root: &Path,
        load: impl FnOnce() -> Result<Configuration>,
    ) -> Result<Configuration> {
        let key = (path.to_path_buf(), project_root.to_path_buf());
        let cached = self.configurations.read().unwrap().get(&key).cloned();
        if let Some(configuration) = cached {
            return Ok((*configuration).clone());
        }

        // Parsing, conversion, and cloning stay outside the lock so unrelated
        // configurations can load in parallel.
        let configuration = Arc::new(load()?);
        let shared = self
            .configurations
            .write()
            .unwrap()
            .entry(key)
            .or_insert_with(|| Arc::clone(&configuration))
            .clone();
        Ok((*shared).clone())
    }
}

/// Recursively resolve a [`Configuration`] from a `pyproject.toml` file at the
/// specified [`Path`].
pub fn resolve_configuration(
    initial_config_path: &Path,
    origin: ConfigurationOrigin<'_>,
) -> Result<Configuration> {
    resolve_configuration_with_cache(initial_config_path, origin, None)
}

fn resolve_configuration_with_cache(
    initial_config_path: &Path,
    origin: ConfigurationOrigin<'_>,
    configuration_cache: Option<&ConfigurationCache>,
) -> Result<Configuration> {
    let relativity = Relativity::from(origin);
    let mut configurations = indexmap::IndexMap::new();
    let mut next = Some(fs::normalize_path(initial_config_path));
    while let Some(path) = next {
        if configurations.contains_key(&path) {
            bail!(format!(
                "Circular configuration detected: {chain}",
                chain = configurations
                    .keys()
                    .chain([&path])
                    .map(|p| format!("`{}`", p.display()))
                    .join(" extends "),
            ));
        }

        let project_root = relativity.resolve(&path);
        let load = || {
            let options = pyproject::load_options(&path).with_context(|| {
                if configurations.is_empty() {
                    format!(
                        "Failed to load configuration `{path}`",
                        path = path.display()
                    )
                } else {
                    let chain = configurations
                        .keys()
                        .chain([&path])
                        .map(|p| format!("`{}`", p.display()))
                        .join(" extends ");
                    format!(
                        "Failed to load extended configuration `{path}` ({chain})",
                        path = path.display()
                    )
                }
            })?;
            Configuration::from_options(options, Some(&path), project_root)
        };
        let configuration = if let Some(cache) = configuration_cache {
            cache.get_or_try_insert_with(&path, project_root, load)?
        } else {
            load()?
        };

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
        configurations.insert(path, configuration);
    }

    // Merge the configurations, in order.
    let mut configurations = configurations.into_values();
    let mut configuration = configurations.next().unwrap();
    for extend in configurations {
        configuration = configuration.combine(extend);
    }

    Ok(configuration.apply_fallbacks(origin, initial_config_path))
}

/// Resolve settings from a configuration file, reusing configuration files shared by the traversal.
fn resolve_scoped_settings(
    pyproject: &Path,
    transformer: &dyn ConfigurationTransformer,
    origin: ConfigurationOrigin<'_>,
    configuration_cache: Option<&ConfigurationCache>,
) -> Result<ConfigurationAndSettings> {
    let configuration = resolve_configuration_with_cache(pyproject, origin, configuration_cache)?;
    ConfigurationAndSettings::new(
        configuration,
        transformer,
        Relativity::from(origin).resolve(pyproject),
        Some(pyproject),
    )
}

/// Extract the [`Settings`] from a given `pyproject.toml` and process the
/// configuration with the given [`ConfigurationTransformer`].
pub fn resolve_root_settings(
    pyproject: &Path,
    transformer: &dyn ConfigurationTransformer,
    origin: ConfigurationOrigin<'_>,
) -> Result<ConfigurationAndSettings> {
    resolve_scoped_settings(pyproject, transformer, origin, None)
}

#[derive(Debug, Clone, Copy)]
/// How the configuration is provided.
pub enum ConfigurationOrigin<'a> {
    /// Origin is unknown to the caller
    Unknown,
    /// User specified path to specific configuration file
    UserSpecified,
    /// User-level configuration and the directory from which to infer a Python version.
    UserSettings { fallback_directory: &'a Path },
    /// In parent or higher ancestor directory of path
    Ancestor,
}

impl From<ConfigurationOrigin<'_>> for Relativity {
    fn from(value: ConfigurationOrigin<'_>) -> Self {
        match value {
            ConfigurationOrigin::Unknown => Self::Parent,
            ConfigurationOrigin::UserSpecified => Self::Cwd,
            ConfigurationOrigin::UserSettings { .. } => Self::Cwd,
            ConfigurationOrigin::Ancestor => Self::Parent,
        }
    }
}

/// Find all project files in a set of paths, following configured include/exclude settings.
pub fn project_files_in_path<'a>(
    paths: &[PathBuf],
    project_config: &'a PyprojectDiscovery,
    transformer: &(dyn ConfigurationTransformer + Sync),
) -> Result<(Vec<Result<ResolvedFile, ignore::Error>>, Resolver<'a>)> {
    // Normalize every path (e.g., convert from relative to absolute).
    let mut paths: Vec<PathBuf> = paths.iter().map(fs::normalize_path).unique().collect();

    // Search for `pyproject.toml` files in all parent directories.
    let mut resolver = Resolver::new(project_config);
    let mut seen = FxHashSet::default();
    let configuration_cache = ConfigurationCache::default();

    // Skip the root configuration file, which has already been loaded.
    if let Some(config_path) = &project_config.root.config_path {
        seen.insert(config_path.parent().unwrap());
    }

    if resolver.is_hierarchical() {
        for path in &paths {
            for ancestor in path.ancestors() {
                if seen.insert(ancestor) {
                    if let Some(pyproject) = settings_toml(ancestor)? {
                        let settings = resolve_scoped_settings(
                            &pyproject,
                            transformer,
                            ConfigurationOrigin::Ancestor,
                            Some(&configuration_cache),
                        )?;
                        resolver.add(ancestor, settings);
                        // We found the closest configuration.
                        break;
                    }
                } else {
                    // We already visited this ancestor, we can stop here.
                    break;
                }
            }
        }
    }

    // Check if the paths themselves are excluded.
    if resolver.force_exclude() {
        paths.retain(|path| !is_file_excluded(path, &resolver));
        if paths.is_empty() {
            return Ok((vec![], resolver));
        }
    }

    let (first_path, rest_paths) = paths
        .split_first()
        .ok_or_else(|| anyhow!("Expected at least one path to search for Python files"))?;
    // Create the `WalkBuilder`.
    let mut builder = WalkBuilder::new(first_path);

    if let Ok(cwd) = std::env::current_dir() {
        builder.current_dir(cwd);
    }

    for path in rest_paths {
        builder.add(path);
    }
    builder.standard_filters(resolver.respect_gitignore());
    builder.hidden(false);

    builder.threads(
        std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(12),
    );

    let walker = builder.build_parallel();

    // Run the `WalkParallel` to collect all files.
    let state = WalkPythonFilesState::new(resolver, configuration_cache);
    let mut visitor = PythonFilesVisitorBuilder::new(transformer, &state);
    walker.visit(&mut visitor);

    state.finish()
}

type ResolvedFiles = Vec<Result<ResolvedFile, ignore::Error>>;

struct WalkPythonFilesState<'config> {
    is_hierarchical: bool,
    merged: std::sync::Mutex<(ResolvedFiles, Result<()>)>,
    resolver: RwLock<Resolver<'config>>,
    configuration_cache: ConfigurationCache,
}

impl<'config> WalkPythonFilesState<'config> {
    fn new(resolver: Resolver<'config>, configuration_cache: ConfigurationCache) -> Self {
        Self {
            is_hierarchical: resolver.is_hierarchical(),
            merged: std::sync::Mutex::new((Vec::new(), Ok(()))),
            resolver: RwLock::new(resolver),
            configuration_cache,
        }
    }

    fn finish(self) -> Result<(Vec<Result<ResolvedFile, ignore::Error>>, Resolver<'config>)> {
        let (files, error) = self.merged.into_inner().unwrap();
        error?;

        let deduplicated_files = deduplicate_files(files);

        Ok((deduplicated_files, self.resolver.into_inner().unwrap()))
    }
}

/// Deduplicate files by path, prioritizing `Root` files over `Nested` files.
///
/// When the same path appears both as a directly specified input (`Root`)
/// and via directory traversal (`Nested`), keep the `Root` entry and drop
/// the `Nested` entry.
///
/// Dropping the root entry means that the explicitly passed path may be
/// unintentionally ignored, since it is treated as nested and can be excluded
/// despite being requested.
///
/// Concretely, with `lint.exclude = ["foo.py"]` and `ruff check . foo.py`,
/// we must keep `Root(foo.py)` and drop `Nested(foo.py)` so `foo.py` is
/// linted as the user requested.
fn deduplicate_files(mut files: ResolvedFiles) -> ResolvedFiles {
    // Sort by path; for identical paths, prefer Root over Nested; place errors after files
    files.sort_by(|a, b| match (a, b) {
        (Ok(a_file), Ok(b_file)) => a_file.cmp(b_file),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        (Err(_), Err(_)) => Ordering::Equal,
    });

    files.dedup_by(|a, b| match (a, b) {
        (Ok(a_file), Ok(b_file)) => a_file.path() == b_file.path(),
        _ => false,
    });

    files
}

struct PythonFilesVisitorBuilder<'s, 'config> {
    state: &'s WalkPythonFilesState<'config>,
    transformer: &'s (dyn ConfigurationTransformer + Sync),
}

impl<'s, 'config> PythonFilesVisitorBuilder<'s, 'config> {
    fn new(
        transformer: &'s (dyn ConfigurationTransformer + Sync),
        state: &'s WalkPythonFilesState<'config>,
    ) -> Self {
        Self { state, transformer }
    }
}

struct PythonFilesVisitor<'s, 'config> {
    local_files: Vec<Result<ResolvedFile, ignore::Error>>,
    local_error: Result<()>,
    global: &'s WalkPythonFilesState<'config>,
    transformer: &'s (dyn ConfigurationTransformer + Sync),
}

impl<'config, 's> ignore::ParallelVisitorBuilder<'s> for PythonFilesVisitorBuilder<'s, 'config>
where
    'config: 's,
{
    fn build(&mut self) -> Box<dyn ignore::ParallelVisitor + 's> {
        Box::new(PythonFilesVisitor {
            local_files: vec![],
            local_error: Ok(()),
            global: self.state,
            transformer: self.transformer,
        })
    }
}

impl ParallelVisitor for PythonFilesVisitor<'_, '_> {
    fn visit(&mut self, result: std::result::Result<DirEntry, Error>) -> WalkState {
        // Respect our own exclusion behavior.
        if let Ok(entry) = &result {
            if entry.depth() > 0 {
                let path = entry.path();
                let resolver = self.global.resolver.read().unwrap();
                let settings = resolver.resolve(path);
                if let Some(file_name) = path.file_name() {
                    let file_path = Candidate::new(path);
                    let file_basename = Candidate::new(file_name);
                    if match_candidate_exclusion(
                        &file_path,
                        &file_basename,
                        &settings.file_resolver.exclude,
                    ) {
                        debug!("Ignored path via `exclude`: {path:?}");
                        return WalkState::Skip;
                    } else if match_candidate_exclusion(
                        &file_path,
                        &file_basename,
                        &settings.file_resolver.extend_exclude,
                    ) {
                        debug!("Ignored path via `extend-exclude`: {path:?}");
                        return WalkState::Skip;
                    }
                } else {
                    debug!("Ignored path due to error in parsing: {path:?}");
                    return WalkState::Skip;
                }
            }
        }

        // Search for the `pyproject.toml` file in this directory, before we visit any
        // of its contents.
        if self.global.is_hierarchical {
            if let Ok(entry) = &result {
                if entry
                    .file_type()
                    .is_some_and(|file_type| file_type.is_dir())
                {
                    match settings_toml(entry.path()) {
                        Ok(Some(pyproject)) => match resolve_scoped_settings(
                            &pyproject,
                            self.transformer,
                            ConfigurationOrigin::Ancestor,
                            Some(&self.global.configuration_cache),
                        ) {
                            Ok(settings) => {
                                self.global
                                    .resolver
                                    .write()
                                    .unwrap()
                                    .add(entry.path(), settings);
                            }
                            Err(err) => {
                                self.local_error = Err(err);
                                return WalkState::Quit;
                            }
                        },
                        Ok(None) => {}
                        Err(err) => {
                            self.local_error = Err(err);
                            return WalkState::Quit;
                        }
                    }
                }
            }
        }

        match result {
            Ok(entry) => {
                // Ignore directories
                let resolved = if entry.file_type().is_none_or(|ft| ft.is_dir()) {
                    None
                } else if entry.depth() == 0 {
                    // Accept all files that are passed-in directly.
                    Some(ResolvedFile::Root(entry.into_path()))
                } else {
                    // Otherwise, check if the file is included.
                    let path = entry.path();
                    let resolver = self.global.resolver.read().unwrap();
                    let settings = resolver.resolve(path);
                    if settings.file_resolver.include.is_match(path) {
                        debug!("Included path via `include`: {path:?}");
                        Some(ResolvedFile::Nested(entry.into_path()))
                    } else if settings.file_resolver.extend_include.is_match(path) {
                        debug!("Included path via `extend-include`: {path:?}");
                        Some(ResolvedFile::Nested(entry.into_path()))
                    } else {
                        None
                    }
                };

                if let Some(resolved) = resolved {
                    self.local_files.push(Ok(resolved));
                }
            }
            Err(err) => {
                self.local_files.push(Err(err));
            }
        }

        WalkState::Continue
    }
}

impl Drop for PythonFilesVisitor<'_, '_> {
    fn drop(&mut self) {
        let mut merged = self.global.merged.lock().unwrap();
        let (files, error) = &mut *merged;

        if files.is_empty() {
            *files = std::mem::take(&mut self.local_files);
        } else {
            files.append(&mut self.local_files);
        }

        let local_error = std::mem::replace(&mut self.local_error, Ok(()));
        if error.is_ok() {
            *error = local_error;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub enum ResolvedFile {
    /// File explicitly passed to the CLI
    Root(PathBuf),
    /// File in a sub-directory
    Nested(PathBuf),
}

impl ResolvedFile {
    pub fn into_path(self) -> PathBuf {
        match self {
            ResolvedFile::Root(path) => path,
            ResolvedFile::Nested(path) => path,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            ResolvedFile::Root(root) => root.as_path(),
            ResolvedFile::Nested(root) => root.as_path(),
        }
    }

    pub fn file_name(&self) -> &OsStr {
        let path = self.path();
        path.file_name().unwrap_or(path.as_os_str())
    }

    pub fn is_root(&self) -> bool {
        matches!(self, ResolvedFile::Root(_))
    }
}

/// Return `true` if the Python file at [`Path`] is _not_ excluded.
pub fn project_file_at_path(
    path: &Path,
    resolver: &mut Resolver,
    transformer: &dyn ConfigurationTransformer,
) -> Result<bool> {
    // Normalize the path (e.g., convert from relative to absolute).
    let path = fs::normalize_path(path);

    // Search for `pyproject.toml` files in all parent directories.
    if resolver.is_hierarchical() {
        for ancestor in path.ancestors() {
            if let Some(pyproject) = settings_toml(ancestor)? {
                let settings = resolve_scoped_settings(
                    &pyproject,
                    transformer,
                    ConfigurationOrigin::Unknown,
                    None,
                )?;
                resolver.add(ancestor, settings);
                break;
            }
        }
    }

    // Check exclusions.
    Ok(!is_file_excluded(&path, resolver))
}

/// Return `true` if the given top-level [`Path`] should be excluded.
fn is_file_excluded(path: &Path, resolver: &Resolver) -> bool {
    // TODO(charlie): Respect gitignore.
    for path in path.ancestors() {
        let settings = resolver.resolve(path);
        if let Some(file_name) = path.file_name() {
            let file_path = Candidate::new(path);
            let file_basename = Candidate::new(file_name);
            if match_candidate_exclusion(
                &file_path,
                &file_basename,
                &settings.file_resolver.exclude,
            ) {
                debug!("Ignored path via `exclude`: {path:?}");
                return true;
            } else if match_candidate_exclusion(
                &file_path,
                &file_basename,
                &settings.file_resolver.extend_exclude,
            ) {
                debug!("Ignored path via `extend-exclude`: {path:?}");
                return true;
            }
        } else {
            break;
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
#[inline]
pub fn match_exclusion<P: AsRef<Path>, R: AsRef<Path>>(
    file_path: P,
    file_basename: R,
    exclusion: &GlobSet,
) -> bool {
    match_candidate_exclusion(
        &Candidate::new(file_path.as_ref()),
        &Candidate::new(file_basename.as_ref()),
        exclusion,
    )
}

/// Return `true` if the given candidates should be ignored based on the exclusion
/// criteria.
fn match_candidate_exclusion(
    file_path: &Candidate,
    file_basename: &Candidate,
    exclusion: &GlobSet,
) -> bool {
    if exclusion.is_empty() {
        return false;
    }
    exclusion.is_match_candidate(file_path) || exclusion.is_match_candidate(file_basename)
}

#[derive(Debug, Copy, Clone)]
pub enum ExclusionKind {
    /// The exclusion came from the `exclude` setting.
    Exclude,
    /// The exclusion came from the `extend-exclude` setting.
    ExtendExclude,
    /// The exclusion came from the `lint.exclude` setting.
    LintExclude,
    /// The exclusion came from the `lint.extend-exclude` setting.
    FormatExclude,
}

impl std::fmt::Display for ExclusionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExclusionKind::Exclude => write!(f, "exclude"),
            ExclusionKind::ExtendExclude => write!(f, "extend-exclude"),
            ExclusionKind::LintExclude => write!(f, "lint.exclude"),
            ExclusionKind::FormatExclude => write!(f, "lint.extend-exclude"),
        }
    }
}

/// Return the [`ExclusionKind`] for a given [`Path`], if the path or any of its ancestors match
/// any of the exclusion criteria.
pub fn match_any_exclusion(
    path: &Path,
    resolver_settings: &FileResolverSettings,
    lint_exclude: Option<&GlobSet>,
    format_exclude: Option<&GlobSet>,
) -> Option<ExclusionKind> {
    for path in path.ancestors() {
        if let Some(basename) = path.file_name() {
            let path = Candidate::new(path);
            let basename = Candidate::new(basename);
            if match_candidate_exclusion(&path, &basename, &resolver_settings.exclude) {
                return Some(ExclusionKind::Exclude);
            }
            if match_candidate_exclusion(&path, &basename, &resolver_settings.extend_exclude) {
                return Some(ExclusionKind::ExtendExclude);
            }
            if let Some(lint_exclude) = lint_exclude {
                if match_candidate_exclusion(&path, &basename, lint_exclude) {
                    return Some(ExclusionKind::LintExclude);
                }
            }
            if let Some(format_exclude) = format_exclude {
                if match_candidate_exclusion(&path, &basename, format_exclude) {
                    return Some(ExclusionKind::FormatExclude);
                }
            }
        }
        if path == resolver_settings.project_root {
            // Bail out; we'd end up past the project root on the next iteration
            // (excludes etc. are thus "rooted" to the project).
            break;
        }
    }
    None
}

#[derive(Debug, Copy, Clone)]
pub enum InclusionKind {
    /// The inclusion came from the `include` setting.
    Include,
    /// The inclusion came from the `extend-include` setting.
    ExtendInclude,
}

impl std::fmt::Display for InclusionKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InclusionKind::Include => write!(f, "include"),
            InclusionKind::ExtendInclude => write!(f, "extend-include"),
        }
    }
}

/// Return the [`InclusionKind`] for a given [`Path`], if the path match any of the inclusion
/// criteria.
pub fn match_any_inclusion(
    path: &Path,
    resolver_settings: &FileResolverSettings,
) -> Option<InclusionKind> {
    if resolver_settings.include.is_match(path) {
        Some(InclusionKind::Include)
    } else if resolver_settings.extend_include.is_match(path) {
        Some(InclusionKind::ExtendInclude)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{File, create_dir};
    use std::path::Path;

    use anyhow::Result;
    use globset::GlobSet;
    use itertools::Itertools;
    use path_absolutize::Absolutize;
    use tempfile::TempDir;

    use ruff_linter::settings::{
        TargetVersion,
        types::{FilePattern, GlobPath, PythonVersion},
    };

    use crate::configuration::Configuration;
    use crate::pyproject::find_settings_toml;
    use crate::resolver::{
        ConfigurationAndSettings, ConfigurationOrigin, ConfigurationTransformer,
        PyprojectDiscovery, PyprojectDiscoveryStrategy, ResolvedFile, Resolver, is_file_excluded,
        match_exclusion, project_files_in_path, resolve_root_settings,
    };
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
        let project_config = PyprojectDiscovery {
            strategy: PyprojectDiscoveryStrategy::Hierarchical,
            root: resolve_root_settings(
                &find_settings_toml(&package_root)?.unwrap(),
                &NoOpTransformer,
                ConfigurationOrigin::Ancestor,
            )?,
        };
        let resolver = Resolver::new(&project_config);
        // src/app.py should not be excluded even if it lives in a hierarchy that should
        // be excluded by virtue of the pyproject.toml having `resources/*` in
        // it.
        assert!(!is_file_excluded(
            &package_root.join("src/app.py"),
            &resolver,
        ));
        // However, resources/ignored.py should be ignored, since that `resources` is
        // beneath the package root.
        assert!(is_file_excluded(
            &package_root.join("resources/ignored.py"),
            &resolver,
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

        let (paths, _) = project_files_in_path(
            &[root.to_path_buf()],
            &PyprojectDiscovery {
                strategy: PyprojectDiscoveryStrategy::Fixed,
                root: ConfigurationAndSettings::default(),
            },
            &NoOpTransformer,
        )?;
        let paths = paths
            .into_iter()
            .flatten()
            .map(ResolvedFile::into_path)
            .sorted()
            .collect::<Vec<_>>();
        assert_eq!(paths, [file2, file1]);

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

        let path = Path::new("foo").absolutize_from(project_root);
        let exclude =
            FilePattern::User("foo".to_string(), GlobPath::normalize("foo", project_root));
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root);
        let exclude =
            FilePattern::User("bar".to_string(), GlobPath::normalize("bar", project_root));
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar/baz.py").absolutize_from(project_root);
        let exclude = FilePattern::User(
            "baz.py".to_string(),
            GlobPath::normalize("baz.py", project_root),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar").absolutize_from(project_root);
        let exclude = FilePattern::User(
            "foo/bar".to_string(),
            GlobPath::normalize("foo/bar", project_root),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar/baz.py").absolutize_from(project_root);
        let exclude = FilePattern::User(
            "foo/bar/baz.py".to_string(),
            GlobPath::normalize("foo/bar/baz.py", project_root),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar/baz.py").absolutize_from(project_root);
        let exclude = FilePattern::User(
            "foo/bar/*.py".to_string(),
            GlobPath::normalize("foo/bar/*.py", project_root),
        );
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));

        let path = Path::new("foo/bar/baz.py").absolutize_from(project_root);
        let exclude =
            FilePattern::User("baz".to_string(), GlobPath::normalize("baz", project_root));
        let file_path = &path;
        let file_basename = path.file_name().unwrap();
        assert!(!match_exclusion(
            file_path,
            file_basename,
            &make_exclusion(exclude),
        ));
    }

    #[test]
    fn project_requires_python_overrides_user_target_version() -> Result<()> {
        let user = TempDir::new()?;
        let config = user.path().join("ruff.toml");
        std::fs::write(&config, "target-version = 'py39'")?;
        let project = TempDir::new()?;
        std::fs::write(
            project.path().join("pyproject.toml"),
            "[project]\nrequires-python = '>=3.12'",
        )?;

        let resolved = resolve_root_settings(
            &config,
            &NoOpTransformer,
            ConfigurationOrigin::UserSettings {
                fallback_directory: project.path(),
            },
        )?;
        assert_eq!(
            resolved.configuration.target_version,
            Some(PythonVersion::Py312.into())
        );
        Ok(())
    }

    #[test]
    fn extend_respects_target_version() -> Result<()> {
        let tmp_dir = TempDir::new()?;
        let root = tmp_dir.path();

        let ruff_toml = root.join("ruff.toml");
        std::fs::write(&ruff_toml, "target-version = \"py310\"")?;

        let dot_ruff_toml = root.join(".ruff.toml");
        std::fs::write(&dot_ruff_toml, "extend = \"ruff.toml\"")?;

        let pyproject_toml = root.join("pyproject.toml");
        std::fs::write(
            &pyproject_toml,
            r#"[project]
name = "repro-ruff"
version = "0.1.0"
requires-python = ">=3.13"
"#,
        )?;

        let main_py = root.join("main.py");
        std::fs::write(
            &main_py,
            r#"from typing import TypeAlias

A: TypeAlias = str | int
"#,
        )?;

        let resolved = resolve_root_settings(
            &dot_ruff_toml,
            &NoOpTransformer,
            ConfigurationOrigin::Ancestor,
        )?;
        assert_eq!(
            resolved.settings.linter.unresolved_target_version,
            TargetVersion(Some(PythonVersion::Py310.into()))
        );

        Ok(())
    }
}
