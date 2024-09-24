use crate::args::{AnalyzeGraphArgs, ConfigArguments};
use crate::resolve::resolve;
use crate::{resolve_default_files, ExitStatus};
use anyhow::Result;
use log::{debug, warn};
use path_absolutize::CWD;
use ruff_db::system::{SystemPath, SystemPathBuf};
use ruff_graph::{Direction, ImportMap, ModuleDb, ModuleImports};
use ruff_linter::{warn_user, warn_user_once};
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{match_exclusion, python_files_in_path, ResolvedFile};
use rustc_hash::FxHashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Generate an import map.
pub(crate) fn analyze_graph(
    args: AnalyzeGraphArgs,
    config_arguments: &ConfigArguments,
) -> Result<ExitStatus> {
    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside the hierarchy.
    let pyproject_config = resolve(config_arguments, None)?;
    if pyproject_config.settings.analyze.preview.is_disabled() {
        warn_user!("`ruff analyze graph` is experimental and may change without warning");
    }

    // Write all paths relative to the current working directory.
    let root =
        SystemPathBuf::from_path_buf(CWD.clone()).expect("Expected a UTF-8 working directory");

    // Find all Python files.
    let files = resolve_default_files(args.files, false);
    let (paths, resolver) = python_files_in_path(&files, &pyproject_config, config_arguments)?;

    if paths.is_empty() {
        warn_user_once!("No Python files found under the given path(s)");
        return Ok(ExitStatus::Success);
    }

    // Resolve all package roots.
    let package_roots = resolver
        .package_roots(
            &paths
                .iter()
                .flatten()
                .map(ResolvedFile::path)
                .collect::<Vec<_>>(),
        )
        .into_iter()
        .map(|(path, package)| (path.to_path_buf(), package.map(Path::to_path_buf)))
        .collect::<FxHashMap<_, _>>();

    // Create a database from the source roots.
    let db = ModuleDb::from_src_roots(
        package_roots
            .values()
            .filter_map(|package| package.as_deref())
            .filter_map(|package| package.parent())
            .map(Path::to_path_buf)
            .filter_map(|path| SystemPathBuf::from_path_buf(path).ok()),
        pyproject_config
            .settings
            .analyze
            .target_version
            .as_tuple()
            .into(),
    )?;

    let imports = {
        // Create a cache for resolved globs.
        let glob_resolver = Arc::new(Mutex::new(GlobResolver::default()));

        // Collect and resolve the imports for each file.
        let result = Arc::new(Mutex::new(Vec::new()));
        let inner_result = Arc::clone(&result);
        let db = db.snapshot();

        rayon::scope(move |scope| {
            for resolved_file in paths {
                let Ok(resolved_file) = resolved_file else {
                    continue;
                };

                let path = resolved_file.path();
                let package = path
                    .parent()
                    .and_then(|parent| package_roots.get(parent))
                    .and_then(Clone::clone);

                // Resolve the per-file settings.
                let settings = resolver.resolve(path);
                let string_imports = settings.analyze.detect_string_imports;
                let include_dependencies = settings.analyze.include_dependencies.get(path).cloned();

                // Skip excluded files.
                if (settings.file_resolver.force_exclude || !resolved_file.is_root())
                    && match_exclusion(
                        resolved_file.path(),
                        resolved_file.file_name(),
                        &settings.analyze.exclude,
                    )
                {
                    continue;
                }

                // Ignore non-Python files.
                let source_type = match settings.analyze.extension.get(path) {
                    None => match SourceType::from(&path) {
                        SourceType::Python(source_type) => source_type,
                        SourceType::Toml(_) => {
                            debug!("Ignoring TOML file: {}", path.display());
                            continue;
                        }
                    },
                    Some(language) => PySourceType::from(language),
                };
                if matches!(source_type, PySourceType::Ipynb) {
                    debug!("Ignoring Jupyter notebook: {}", path.display());
                    continue;
                }

                // Convert to system paths.
                let Ok(package) = package.map(SystemPathBuf::from_path_buf).transpose() else {
                    warn!("Failed to convert package to system path");
                    continue;
                };
                let Ok(path) = SystemPathBuf::from_path_buf(resolved_file.into_path()) else {
                    warn!("Failed to convert path to system path");
                    continue;
                };

                let db = db.snapshot();
                let glob_resolver = glob_resolver.clone();
                let root = root.clone();
                let result = inner_result.clone();
                scope.spawn(move |_| {
                    // Identify any imports via static analysis.
                    let mut imports =
                        ModuleImports::detect(&db, &path, package.as_deref(), string_imports)
                            .unwrap_or_else(|err| {
                                warn!("Failed to generate import map for {path}: {err}");
                                ModuleImports::default()
                            });

                    debug!("Discovered {} imports for {}", imports.len(), path);

                    // Append any imports that were statically defined in the configuration.
                    if let Some((root, globs)) = include_dependencies {
                        let mut glob_resolver = glob_resolver.lock().unwrap();
                        imports.extend(glob_resolver.resolve(root, globs));
                    }

                    // Convert the path (and imports) to be relative to the working directory.
                    let path = path
                        .strip_prefix(&root)
                        .map(SystemPath::to_path_buf)
                        .unwrap_or(path);
                    let imports = imports.relative_to(&root);

                    result.lock().unwrap().push((path, imports));
                });
            }
        });

        // Collect the results.
        Arc::into_inner(result).unwrap().into_inner()?
    };

    // Generate the import map.
    let import_map = match args.direction {
        Direction::Dependencies => ImportMap::dependencies(imports),
        Direction::Dependents => ImportMap::dependents(imports),
    };

    // Print to JSON.
    writeln!(
        std::io::stdout(),
        "{}",
        serde_json::to_string_pretty(&import_map)?
    )?;

    std::mem::forget(db);

    Ok(ExitStatus::Success)
}

/// A resolver for glob sets.
#[derive(Default, Debug)]
struct GlobResolver {
    cache: GlobCache,
}

impl GlobResolver {
    /// Resolve a set of globs, anchored at a given root.
    fn resolve(&mut self, root: PathBuf, globs: Vec<String>) -> Vec<SystemPathBuf> {
        if let Some(cached) = self.cache.get(&root, &globs) {
            return cached.clone();
        }

        let walker = match globwalk::GlobWalkerBuilder::from_patterns(&root, &globs)
            .file_type(globwalk::FileType::FILE)
            .build()
        {
            Ok(walker) => walker,
            Err(err) => {
                warn!("Failed to read glob walker: {err}");
                return Vec::new();
            }
        };

        let mut paths = Vec::new();
        for entry in walker {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    warn!("Failed to read glob entry: {err}");
                    continue;
                }
            };
            let path = match SystemPathBuf::from_path_buf(entry.into_path()) {
                Ok(path) => path,
                Err(err) => {
                    warn!("Failed to convert path to system path: {}", err.display());
                    continue;
                }
            };
            paths.push(path);
        }

        self.cache.insert(root, globs, paths.clone());
        paths
    }
}

/// A cache for resolved globs.
#[derive(Default, Debug)]
struct GlobCache(FxHashMap<PathBuf, FxHashMap<Vec<String>, Vec<SystemPathBuf>>>);

impl GlobCache {
    /// Insert a resolved glob.
    fn insert(&mut self, root: PathBuf, globs: Vec<String>, paths: Vec<SystemPathBuf>) {
        self.0.entry(root).or_default().insert(globs, paths);
    }

    /// Get a resolved glob.
    fn get(&self, root: &Path, globs: &[String]) -> Option<&Vec<SystemPathBuf>> {
        self.0.get(root).and_then(|map| map.get(globs))
    }
}
