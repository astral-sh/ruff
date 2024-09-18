use crate::args::{ConfigArguments, ImportMapArgs};
use crate::resolve::resolve;
use crate::{resolve_default_files, ExitStatus};
use colored::Colorize;
use ignore::Error;
use log::{debug, warn};
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use ruff_import_map::{Direction, ImportMap, ModuleDb};
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{python_files_in_path, ResolvedFile};
use std::fmt::Write;
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

    // Discover the package root for each Python file.
    let package_roots = resolver.package_roots(
        &paths
            .iter()
            .flatten()
            .map(ResolvedFile::path)
            .collect::<Vec<_>>(),
    );

    // // Initialize the module database.
    // let settings = resolver.settings().next().expect("settings");
    // let db = ModuleDb::from_settings(&settings.import_map)?;

    // (1) Collect the imports for each file.
    let imports = paths
        .par_iter()
        .filter_map(|resolved_file| {
            let result = match resolved_file {
                Ok(resolved_file) => {
                    let path = resolved_file.path();
                    let package = path
                        .parent()
                        .and_then(|parent| package_roots.get(parent))
                        .and_then(|package| *package);

                    let settings = resolver.resolve(path);

                    // Ignore non-Python files.
                    let source_type = match settings.import_map.extension.get(path) {
                        None => match SourceType::from(path) {
                            SourceType::Python(source_type) => source_type,
                            SourceType::Toml(_) => {
                                return None;
                            }
                        },
                        Some(language) => PySourceType::from(language),
                    };
                    if !matches!(source_type, PySourceType::Python | PySourceType::Stub) {
                        return None;
                    }

                    let imports = ruff_import_map::generate(
                        path,
                        package,
                        source_type,
                        &settings.import_map,
                        // &db,
                    )
                    .map_err(|e| {
                        (Some(path.to_path_buf()), {
                            let mut error = e.to_string();
                            for cause in e.chain() {
                                write!(&mut error, "\n  Cause: {cause}").unwrap();
                            }
                            error
                        })
                    });

                    imports.map(|imports| (path.to_path_buf(), imports))
                }
                Err(e) => Err((
                    if let Error::WithPath { path, .. } = e {
                        Some(path.clone())
                    } else {
                        None
                    },
                    e.io_error()
                        .map_or_else(|| e.to_string(), std::io::Error::to_string),
                )),
            };

            match result {
                Ok(imports) => Some(imports),
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
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    let import_map = match pyproject_config.settings.import_map.direction {
        Direction::Dependencies => ImportMap::from_iter(imports),
        Direction::Dependents => ImportMap::reverse(imports),
    };

    // Print to JSON.

    println!("{}", serde_json::to_string_pretty(&import_map)?);

    Ok(ExitStatus::Success)
}
