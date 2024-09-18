use crate::args::{ConfigArguments, ImportMapArgs};
use crate::resolve::resolve;
use crate::{resolve_default_files, ExitStatus};
use log::{debug, warn};
use ruff_import_map::{Direction, ImportMap, ModuleDb};
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{python_files_in_path, ResolvedFile};
use rustc_hash::FxHashMap;
use std::collections::BTreeSet;
use std::fmt::Write;
use std::sync::Arc;
use std::time::Instant;

pub(crate) fn import_map(
    args: ImportMapArgs,
    config_arguments: &ConfigArguments,
) -> anyhow::Result<ExitStatus> {
    let start = Instant::now();

    // Construct the "default" settings. These are used when no `pyproject.toml`
    // files are present, or files are injected from outside of the hierarchy.
    let pyproject_config = resolve(config_arguments, None)?;

    // Find all Python files.
    let files = resolve_default_files(args.files, false);
    let (paths, resolver) = python_files_in_path(&files, &pyproject_config, config_arguments)?;

    debug!("Identified files to lint in: {:?}", start.elapsed());

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
        .map(|(path, package)| {
            (
                path.to_path_buf(),
                package.map(std::path::Path::to_path_buf),
            )
        })
        .collect::<FxHashMap<_, _>>();

    // Infer all source roots.
    let source_roots = package_roots
        .values()
        .filter_map(|package| package.as_deref())
        .filter_map(|package| package.parent())
        .map(std::path::Path::to_path_buf)
        .collect::<BTreeSet<_>>();
    if source_roots.is_empty() {
        warn_user_once!("No Python packages found under the given path(s)");
        return Ok(ExitStatus::Success);
    }

    // Initialize the module database.
    let db = ModuleDb::from_settings(source_roots)?;

    // Collect and resolve the imports for each file.
    let result = Arc::new(std::sync::Mutex::new(Vec::new()));
    let inner_result = Arc::clone(&result);

    rayon::scope(move |scope| {
        for resolved_file in &paths {
            let result = inner_result.clone();
            let db = db.snapshot();

            let Ok(resolved_file) = resolved_file else {
                continue;
            };

            let path = resolved_file.path().to_path_buf();
            let package = path
                .parent()
                .and_then(|parent| package_roots.get(parent))
                .and_then(std::clone::Clone::clone);

            let settings = resolver.resolve(&path);

            // Ignore non-Python files.
            let source_type = match settings.import_map.extension.get(&path) {
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

            scope.spawn(move |_| {
                let imports =
                    ruff_import_map::generate(&path, package.as_deref(), &db).map_err(|e| {
                        (Some(path.clone()), {
                            let mut error = e.to_string();
                            for cause in e.chain() {
                                write!(&mut error, "\n  Cause: {cause}").unwrap();
                            }
                            error
                        })
                    });

                match imports {
                    Ok(imports) => {
                        result.lock().unwrap().push((path, imports));
                    }
                    Err((path, error)) => {
                        if let Some(path) = path {
                            warn!(
                                "Failed to generate import map for {path}: {error}",
                                path = path.display(),
                                error = error
                            );
                        } else {
                            warn!("Failed to generate import map: {error}", error = error);
                        }
                    }
                }
            });
        }
    });

    let imports = Arc::into_inner(result).unwrap().into_inner()?;
    let import_map = match pyproject_config.settings.import_map.direction {
        Direction::Dependencies => ImportMap::from_iter(imports),
        Direction::Dependents => ImportMap::reverse(imports),
    };

    // Print to JSON.
    println!("{}", serde_json::to_string_pretty(&import_map)?);

    Ok(ExitStatus::Success)
}
