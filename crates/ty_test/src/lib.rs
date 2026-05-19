use crate::config::{Log, MarkdownTestConfig, SystemKind};
use anyhow::{anyhow, bail};
use camino::Utf8Path;
use mdtest::matcher::{self, Failure};
use mdtest::parser::{self};
use mdtest::{
    Failures, FileFailures, MarkdownEdit, OutputFormat, TestFile, TestOutcome, attempt_test,
};
use ruff_db::Db;
use ruff_db::cancellation::CancellationTokenSource;
use ruff_db::diagnostic::DiagnosticId;
use ruff_db::files::{FileRootKind, system_path_to_file};
use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
use ruff_db::testing::{setup_logging, setup_logging_with_filter};
use ruff_diagnostics::Applicability;
use ruff_source_file::OneIndexed;
use std::fmt::Write;
use ty_module_resolver::{
    Module, SearchPath, SearchPathSettings, list_modules, resolve_module_confident,
};
use ty_python_core::platform::PythonPlatform;
use ty_python_core::program::{FallibleStrategy, Program, ProgramSettings};
use ty_python_semantic::pull_types::pull_types;
use ty_python_semantic::types::UNDEFINED_REVEAL;
use ty_python_semantic::{
    PythonEnvironment, PythonVersionSource, PythonVersionWithSource, SysPrefixPathOrigin,
    fix_all_diagnostics,
};

mod config;
mod db;
mod external_dependencies;

/// If set to a value other than "0", runs tests that include external dependencies.
const MDTEST_EXTERNAL: &str = "MDTEST_EXTERNAL";

/// Run `path` as a markdown test suite with given `title`.
///
/// Panic on test failure, and print failure details.
pub fn run(
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    source: &str,
    snapshot_path: &Utf8Path,
    short_title: &str,
    test_name: &str,
) -> anyhow::Result<()> {
    let mut db = db::Db::setup();

    let suite =
        parse(short_title, source).map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

    mdtest::run(
        absolute_fixture_path,
        relative_fixture_path,
        source,
        test_name,
        "ty_python_semantic",
        &suite,
        |test, assertion, output_format| {
            run_test(
                &mut db,
                absolute_fixture_path,
                relative_fixture_path,
                snapshot_path,
                test,
                assertion,
                output_format,
            )
        },
    )
}

fn run_test(
    db: &mut db::Db,
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &parser::MarkdownTest<'_, '_, MarkdownTestConfig>,
    assertion: &mut String,
    output_format: OutputFormat,
) -> Result<(TestOutcome, Vec<MarkdownEdit>), Failures> {
    let _tracing = test.configuration().log.as_ref().and_then(|log| match log {
        Log::Bool(enabled) => enabled.then(setup_logging),
        Log::Filter(filter) => setup_logging_with_filter(filter),
    });

    // Initialize the system and remove all files and directories to reset the system to a clean state.
    match test.configuration().system.unwrap_or_default() {
        SystemKind::InMemory => {
            db.use_in_memory_system();
        }
        SystemKind::Os => {
            let dir = tempfile::TempDir::new().expect("Creating a temporary directory to succeed");
            let root_path = dir
                .path()
                .canonicalize()
                .expect("Canonicalizing to succeed");
            let root_path = SystemPathBuf::from_path_buf(root_path)
                .expect("Temp directory to be a valid UTF8 path")
                .simplified()
                .to_path_buf();

            db.use_os_system_with_temp_dir(root_path, dir);
        }
    }

    let project_root = SystemPathBuf::from("/src");
    db.create_directory_all(&project_root)
        .expect("Creating the project root to succeed");
    db.files()
        .try_add_root(db, &project_root, FileRootKind::Project);

    let src_path = project_root.clone();
    let custom_typeshed_path = test.configuration().typeshed();
    let python_version = test.configuration().python_version().unwrap_or_default();

    // Setup virtual environment with dependencies if specified
    let venv_for_external_dependencies = SystemPathBuf::from("/.venv");
    if let Some(dependencies) = test.configuration().dependencies() {
        if !std::env::var(MDTEST_EXTERNAL).is_ok_and(|v| v == "1") {
            return Ok((TestOutcome::Skipped, vec![]));
        }

        let python_platform = test.configuration().python_platform().expect(
            "Tests with external dependencies must specify `python-platform` in the configuration",
        );

        let lockfile_path = absolute_fixture_path.with_extension("lock");

        external_dependencies::setup_venv(
            db,
            dependencies,
            python_version,
            &python_platform,
            &venv_for_external_dependencies,
            &lockfile_path,
        )
        .expect("Failed to setup in-memory virtual environment with dependencies");
    }

    let mut typeshed_files = vec![];
    let mut has_custom_versions_file = false;

    let test_files: Vec<_> = test
        .files()
        .filter_map(|embedded| {
            if embedded.lang == "ignore" {
                return None;
            }

            assert!(
                matches!(
                    embedded.lang,
                    "py" | "pyi" | "python" | "text" | "cfg" | "pth"
                ),
                "Supported file types are: py (or python), pyi, text, cfg and ignore"
            );

            let mut full_path = embedded.full_path(&project_root);

            if let Some(relative_path_to_custom_typeshed) = custom_typeshed_path
                .and_then(|typeshed| full_path.strip_prefix(typeshed.join("stdlib")).ok())
            {
                if relative_path_to_custom_typeshed.as_str() == "VERSIONS" {
                    has_custom_versions_file = true;
                } else if relative_path_to_custom_typeshed
                    .extension()
                    .is_some_and(|ext| ext == "pyi")
                {
                    typeshed_files.push(relative_path_to_custom_typeshed.to_path_buf());
                }
            } else if let Some(component_index) = full_path
                .components()
                .position(|c| c.as_str() == "<path-to-site-packages>")
            {
                // If the path contains `<path-to-site-packages>`, we need to replace it with the
                // actual site-packages directory based on the Python platform and version.
                let mut components = full_path.components();
                let mut new_path: SystemPathBuf =
                    components.by_ref().take(component_index).collect();
                if cfg!(target_os = "windows") {
                    new_path.extend(["Lib", "site-packages"]);
                } else {
                    new_path.push("lib");
                    new_path.push(format!("python{python_version}"));
                    new_path.push("site-packages");
                }
                new_path.extend(components.skip(1));
                full_path = new_path;
            }

            let temp_string;
            let to_write = if embedded.lang == "pth" && !embedded.code.starts_with('/') {
                // Make any relative .pths be relative to src_path
                temp_string = format!("{src_path}/{}", embedded.code);
                &*temp_string
            } else {
                &*embedded.code
            };

            db.write_file(&full_path, to_write).unwrap();

            if !(full_path.starts_with(&src_path)
                && matches!(embedded.lang, "py" | "python" | "pyi"))
            {
                // These files need to be written to the file system (above), but we don't run any checks on them.
                return None;
            }

            let file = system_path_to_file(db, full_path).unwrap();

            Some(TestFile {
                file,
                code_blocks: embedded.python_code_blocks.clone(),
            })
        })
        .collect();

    // Create a custom typeshed `VERSIONS` file if none was provided.
    if let Some(typeshed_path) = custom_typeshed_path {
        db.files()
            .try_add_root(db, typeshed_path, FileRootKind::LibrarySearchPath);
        if !has_custom_versions_file {
            let versions_file = typeshed_path.join("stdlib/VERSIONS");
            let contents = typeshed_files
                .iter()
                .fold(String::new(), |mut content, path| {
                    // This is intentionally kept simple:
                    let module_name = path
                        .as_str()
                        .trim_end_matches(".pyi")
                        .trim_end_matches("/__init__")
                        .replace('/', ".");
                    let _ = writeln!(content, "{module_name}: 3.8-");
                    content
                });
            db.write_file(&versions_file, contents).unwrap();
        }
    }

    let configuration = test.configuration();

    let site_packages_paths = if configuration.dependencies().is_some() {
        // If dependencies were specified, use the venv we just set up
        let environment = PythonEnvironment::new(
            &venv_for_external_dependencies,
            SysPrefixPathOrigin::PythonCliFlag,
            db.system(),
        )
        .expect("Python environment to point to a valid path");
        environment
            .site_packages_paths(db.system())
            .expect("Python environment to be valid")
            .into_vec()
    } else if let Some(python) = configuration.python() {
        let environment =
            PythonEnvironment::new(python, SysPrefixPathOrigin::PythonCliFlag, db.system())
                .expect("Python environment to point to a valid path");
        environment
            .site_packages_paths(db.system())
            .expect("Python environment to be valid")
            .into_vec()
    } else {
        vec![]
    };

    // Make any relative extra-paths be relative to src_path
    let extra_paths = configuration
        .extra_paths()
        .unwrap_or_default()
        .iter()
        .map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                src_path.join(path)
            }
        })
        .collect();

    let settings = ProgramSettings {
        python_version: PythonVersionWithSource {
            version: python_version,
            source: PythonVersionSource::Cli,
        },
        python_platform: configuration
            .python_platform()
            .unwrap_or(PythonPlatform::Identifier("linux".to_string())),
        search_paths: SearchPathSettings {
            src_roots: vec![src_path],
            extra_paths,
            custom_typeshed: custom_typeshed_path.map(SystemPath::to_path_buf),
            site_packages_paths,
            real_stdlib_path: None,
        }
        .to_search_paths(db.system(), db.vendored(), &FallibleStrategy)
        .expect("Failed to resolve search path settings"),
    };

    Program::init_or_update(db, settings);
    db.update_analysis_options(configuration.analysis.as_ref());
    db.set_verbosity(test.configuration().verbose());

    let mut all_diagnostics = vec![];

    // Edits for updating changed inline snapshots.
    let mut markdown_edits = vec![];

    let mut any_pull_types_failures = false;
    let mut panic_info = None;

    let mut failures: Failures = test_files
        .iter()
        .filter_map(|test_file| {
            let mdtest_result = attempt_test(
                |file| ty_python_semantic::Db::check_file(db, file),
                test_file,
            );
            let diagnostics = match mdtest_result {
                Ok(diagnostics) => diagnostics,
                Err(failures) => {
                    if test.should_expect_panic().is_ok() {
                        panic_info = Some(failures.info);
                        return None;
                    }

                    return Some(failures.into_file_failures(db, "run mdtest", None));
                }
            };

            let failure = match matcher::match_file(db, test_file.file, &diagnostics).and_then(
                |inline_diagnostics| {
                    mdtest::validate_inline_snapshot(
                        db,
                        "ty",
                        test_file,
                        &inline_diagnostics,
                        &mut markdown_edits,
                    )
                },
            ) {
                Ok(()) => None,
                Err(line_failures) => Some(FileFailures {
                    backtick_offsets: test_file.to_code_block_backtick_offsets(),
                    by_line: line_failures,
                }),
            };

            all_diagnostics.extend(diagnostics);

            let pull_types_result = attempt_test(|file| pull_types(db, file), test_file);
            match pull_types_result {
                Ok(()) => {}
                Err(failures) => {
                    any_pull_types_failures = true;
                    if !test.should_skip_pulling_types() {
                        return Some(failures.into_file_failures(
                            db,
                            "\"pull types\"",
                            Some(
                                "Note: either fix the panic or add the `<!-- pull-types:skip -->` \
                    directive to this test",
                            ),
                        ));
                    }
                }
            }

            failure
        })
        .collect();

    mdtest::check_panic(test, panic_info);

    if test.should_skip_pulling_types() && !any_pull_types_failures {
        let mut by_line = matcher::FailuresByLine::default();
        by_line.push(
            OneIndexed::from_zero_indexed(0),
            vec![Failure::new(
                "Remove the `<!-- pull-types:skip -->` directive from this test: pulling types \
                 succeeded for all files in the test.",
            )],
        );
        let failure = FileFailures {
            backtick_offsets: test_files[0].to_code_block_backtick_offsets(),
            by_line,
        };
        failures.push(failure);
    }

    // Filter out `revealed-type` and `undefined-reveal` diagnostics from snapshots,
    // since they make snapshots very noisy!
    mdtest::snapshot_diagnostics(
        test,
        db,
        "ty",
        relative_fixture_path,
        snapshot_path,
        &all_diagnostics,
        |diagnostic| {
            diagnostic.id() != DiagnosticId::RevealedType
                && !diagnostic.id().is_lint_named(&UNDEFINED_REVEAL.name())
        },
    );

    // Test to fix all fixable diagnostics and verify that they don't introduce any syntax errors.
    // But don't try to run fixes for tests that are expected to panic.
    if test.should_expect_panic().is_err() {
        let token_source = CancellationTokenSource::new();
        let result = fix_all_diagnostics(
            db,
            all_diagnostics,
            Applicability::Unsafe,
            &token_source.token(),
        )
        .expect("to succeed because fixing is never cancelled");

        tracing::debug!("Fixed {} diagnostics", result.count);

        let mut fatals = result.diagnostics;
        fatals.retain(|diagnostic| diagnostic.id() == DiagnosticId::InternalError);

        for diagnostic in fatals {
            let ty_file = diagnostic.expect_primary_span().expect_ty_file();

            let test_file = test_files
                .iter()
                .find(|test_file| test_file.file == ty_file)
                .unwrap_or(&test_files[0]);

            let mut by_line = matcher::FailuresByLine::default();
            by_line.push(
                OneIndexed::from_zero_indexed(0),
                vec![Failure::new(format_args!(
                    "Fixing the diagnostics caused a fatal error:\n{}",
                    mdtest::render_diagnostic(db, "ty", &diagnostic)
                ))],
            );
            let failure = FileFailures {
                backtick_offsets: test_file.to_code_block_backtick_offsets(),
                by_line,
            };
            failures.push(failure);
        }
    }

    let inconsistencies = run_module_resolution_consistency_test(db);

    if let Err(inconsistencies) = &inconsistencies {
        for inconsistency in inconsistencies {
            output_format.write_inconsistency(assertion, relative_fixture_path, &inconsistency);
        }
    }

    if failures.is_empty() && inconsistencies.is_ok() {
        Ok((TestOutcome::Success, markdown_edits))
    } else {
        Err(failures)
    }
}

/// Reports an inconsistency between "list modules" and "resolve module."
///
/// Values of this type are only constructed when `from_list` and
/// `from_resolve` are not equivalent.
struct ModuleInconsistency<'db> {
    db: &'db db::Db,
    /// The module returned from `list_module`.
    from_list: Module<'db>,
    /// The module returned, if any, from `resolve_module`.
    from_resolve: Option<Module<'db>>,
}

/// Tests that "list modules" is consistent with "resolve module."
///
/// This only checks that everything returned by `list_module` is the
/// identical module we get back from `resolve_module`. It does not
/// check that all possible outputs of `resolve_module` are captured by
/// `list_module`.
fn run_module_resolution_consistency_test(db: &db::Db) -> Result<(), Vec<ModuleInconsistency<'_>>> {
    let mut errs = vec![];
    for from_list in list_modules(db) {
        // TODO: For now list_modules does not partake in desperate module resolution so
        // only compare against confident module resolution.
        errs.push(match resolve_module_confident(db, from_list.name(db)) {
            None => ModuleInconsistency {
                db,
                from_list,
                from_resolve: None,
            },
            Some(from_resolve) if from_list != from_resolve => ModuleInconsistency {
                db,
                from_list,
                from_resolve: Some(from_resolve),
            },
            _ => continue,
        });
    }
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}

impl std::fmt::Display for ModuleInconsistency<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        fn fmt_module(
            db: &db::Db,
            f: &mut std::fmt::Formatter,
            module: &Module<'_>,
        ) -> std::fmt::Result {
            let name = module.name(db);
            let path = module
                .file(db)
                .map(|file| file.path(db).to_string())
                .unwrap_or_else(|| "N/A".to_string());
            let search_path = module
                .search_path(db)
                .map(SearchPath::to_string)
                .unwrap_or_else(|| "N/A".to_string());
            let known = module
                .known(db)
                .map(|known| known.to_string())
                .unwrap_or_else(|| "N/A".to_string());
            write!(
                f,
                "Module(\
                   name={name}, \
                   file={path}, \
                   kind={kind:?}, \
                   search_path={search_path}, \
                   known={known}\
                 )",
                kind = module.kind(db),
            )
        }
        write!(f, "Found ")?;
        fmt_module(self.db, f, &self.from_list)?;
        match self.from_resolve {
            None => write!(
                f,
                " when listing modules, but `resolve_module` returned `None`",
            )?,
            Some(ref got) => {
                write!(f, " when listing modules, but `resolve_module` returned ")?;
                fmt_module(self.db, f, got)?;
            }
        }
        Ok(())
    }
}

fn parse<'s>(
    short_title: &'s str,
    source: &'s str,
) -> anyhow::Result<parser::MarkdownTestSuite<'s, MarkdownTestConfig>> {
    let mut file_has_dependencies = false;
    parser::parse::<MarkdownTestConfig>(short_title, source, |config| {
        if config.dependencies().is_some() {
            if file_has_dependencies {
                bail!(
                    "Multiple sections with `[project]` dependencies in the same file are not allowed. \
                     External dependencies must be specified in a single top-level configuration block."
                );
            }
            file_has_dependencies = true;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use ruff_python_trivia::textwrap::dedent;

    #[test]
    fn multiple_sections_with_dependencies_not_allowed() {
        let source = dedent(
            r#"
            # First section

            ```toml
            [project]
            dependencies = ["pydantic==2.12.2"]
            ```

            ```py
            x = 1
            ```

            # Second section

            ```toml
            [project]
            dependencies = ["numpy==2.0.0"]
            ```

            ```py
            y = 2
            ```
            "#,
        );
        let err = super::parse("file.md", &source).expect_err("Should fail to parse");
        assert_eq!(
            err.to_string(),
            "Multiple sections with `[project]` dependencies in the same file are not allowed. \
             External dependencies must be specified in a single top-level configuration block."
        );
    }
}
