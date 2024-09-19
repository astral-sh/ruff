use crate::args::{ConfigArguments, ImportMapArgs};
use crate::resolve::resolve;
use crate::{resolve_default_files, ExitStatus};
use anyhow::Result;
use log::{debug, warn};
use ruff_graph::{Direction, ImportMap, ModuleDb, ModuleImports};
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{python_files_in_path, ResolvedFile};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

/// Generate an import map.
pub(crate) fn import_map(
    args: ImportMapArgs,
    config_arguments: &ConfigArguments,
) -> Result<ExitStatus> {
    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside the hierarchy.
    let pyproject_config = resolve(config_arguments, None)?;

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

    // Create a database for each source root.
    let databases = package_roots
        .values()
        .filter_map(|package| package.as_deref())
        .filter_map(|package| package.parent())
        .map(Path::to_path_buf)
        .map(|source_root| Ok((source_root.clone(), ModuleDb::from_src_root(source_root)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;

    // Collect and resolve the imports for each file.
    let result = Arc::new(std::sync::Mutex::new(Vec::new()));
    let inner_result = Arc::clone(&result);

    rayon::scope(move |scope| {
        for resolved_file in &paths {
            let Ok(resolved_file) = resolved_file else {
                continue;
            };

            let path = resolved_file.path().to_path_buf();
            let package = path
                .parent()
                .and_then(|parent| package_roots.get(parent))
                .and_then(Clone::clone);
            let Some(src_root) = package
                .as_ref()
                .and_then(|package| package.parent())
                .map(Path::to_path_buf)
            else {
                debug!("Ignoring file outside of source root: {}", path.display());
                continue;
            };
            let Some(db) = databases.get(&src_root).map(ModuleDb::snapshot) else {
                continue;
            };

            // Resolve the per-file settings.
            let settings = resolver.resolve(&path);
            let string_imports = settings.graph.detect_string_imports;
            let include_dependencies = settings.graph.include_dependencies.get(&path).cloned();

            // Ignore non-Python files.
            let source_type = match settings.graph.extension.get(&path) {
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

            let result = inner_result.clone();
            scope.spawn(move |_| {
                // Identify any imports via static analysis.
                let mut imports =
                    ruff_graph::generate(&path, package.as_deref(), string_imports, &db)
                        .unwrap_or_else(|err| {
                            warn!(
                                "Failed to generate import map for {path}: {err}",
                                path = path.display(),
                            );
                            ModuleImports::default()
                        });

                // Append any imports that were statically defined in the configuration.
                if let Some((root, globs)) = include_dependencies {
                    match globwalk::GlobWalkerBuilder::from_patterns(root, &globs)
                        .file_type(globwalk::FileType::FILE)
                        .build()
                    {
                        Ok(walker) => {
                            for entry in walker {
                                let entry = match entry {
                                    Ok(entry) => entry,
                                    Err(err) => {
                                        warn!("Failed to read glob entry: {err}");
                                        continue;
                                    }
                                };
                                imports.insert(entry.into_path());
                            }
                        }
                        Err(err) => {
                            warn!("Failed to read glob walker: {err}");
                        }
                    }
                }

                // Convert the path (and imports) to be relative to the source root.
                let path = path.strip_prefix(&src_root).unwrap_or(&path).to_path_buf();
                let imports = imports.relative_to(&src_root);

                result.lock().unwrap().push((path, imports));
            });
        }
    });

    // Collect the results.
    let imports = Arc::into_inner(result).unwrap().into_inner()?;

    // Generate the import map.
    let import_map = match args.direction {
        Direction::Dependencies => ImportMap::from_iter(imports),
        Direction::Dependents => ImportMap::reverse(imports),
    };

    // Print to JSON.
    println!("{}", serde_json::to_string_pretty(&import_map)?);

    Ok(ExitStatus::Success)
}
