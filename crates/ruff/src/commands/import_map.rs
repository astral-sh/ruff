use crate::args::{ConfigArguments, GlobalConfigArgs, ImportMapArgs, ImportMapCommand};
use crate::cache::PackageCacheMap;
use crate::diagnostics::Diagnostics;
use crate::resolve::resolve;
use crate::{resolve_default_files, ExitStatus};
use ignore::Error;
use log::debug;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use std::fmt::Write;

use ruff_import_map::ImportMapSettings;
use ruff_linter::warn_user_once;
use ruff_python_ast::{PySourceType, SourceType};
use ruff_workspace::resolver::{match_exclusion, python_files_in_path, ResolvedFile};
use std::time::Instant;

pub fn import_map(
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

    // (1) Collect the imports for each file.
    paths.par_iter().for_each(|resolved_file| {
        let result = match resolved_file {
            Ok(resolved_file) => {
                let path = resolved_file.path();
                let package = path
                    .parent()
                    .and_then(|parent| package_roots.get(parent))
                    .and_then(|package| *package);

                let settings = resolver.resolve(path);

                if (settings.file_resolver.force_exclude || !resolved_file.is_root())
                    && match_exclusion(
                        resolved_file.path(),
                        resolved_file.file_name(),
                        &settings.linter.exclude,
                    )
                {
                    return;
                }

                // Ignore non-Python files.
                let source_type = match settings.linter.extension.get(path) {
                    None => match SourceType::from(path) {
                        SourceType::Python(source_type) => source_type,
                        SourceType::Toml(_) => {
                            return;
                        }
                    },
                    Some(language) => PySourceType::from(language),
                };
                if !matches!(source_type, PySourceType::Python | PySourceType::Stub) {
                    return;
                }

                ruff_import_map::generate(path, package, source_type, &ImportMapSettings::default())
                    .map_err(|e| {
                        (Some(path.to_path_buf()), {
                            let mut error = e.to_string();
                            for cause in e.chain() {
                                write!(&mut error, "\n  Cause: {cause}").unwrap();
                            }
                            error
                        })
                    })
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

        // Some(result.unwrap_or_else(|(path, message)| {
        //     if let Some(path) = &path {
        //         let settings = resolver.resolve(path);
        //         if settings.linter.rules.enabled(Rule::IOError) {
        //             let dummy =
        //                 SourceFileBuilder::new(path.to_string_lossy().as_ref(), "").finish();
        //
        //             Diagnostics::new(
        //                 vec![Message::from_diagnostic(
        //                     Diagnostic::new(IOError { message }, TextRange::default()),
        //                     dummy,
        //                     TextSize::default(),
        //                 )],
        //                 FxHashMap::default(),
        //             )
        //         } else {
        //             warn!(
        //                 "{}{}{} {message}",
        //                 "Failed to lint ".bold(),
        //                 fs::relativize_path(path).bold(),
        //                 ":".bold()
        //             );
        //             Diagnostics::default()
        //         }
        //     } else {
        //         warn!("{} {message}", "Encountered error:".bold());
        //         Diagnostics::default()
        //     }
        // }))
    });
    // (2) Map each import to a file.

    // (3) Write the import map to a file.

    Ok(ExitStatus::Success)
}
