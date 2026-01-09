use crate::config::Log;
use crate::db::Db;
use crate::parser::{BacktickOffsets, EmbeddedFileSourceMap};
use anyhow::anyhow;
use camino::Utf8Path;
use colored::Colorize;
use config::SystemKind;
use parser as test_parser;
use ruff_db::Db as _;
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, DisplayDiagnosticConfig};
use ruff_db::files::{File, FileRootKind, system_path_to_file};
use ruff_db::panic::{PanicError, catch_unwind};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{DbWithWritableSystem as _, SystemPath, SystemPathBuf};
use ruff_db::testing::{setup_logging, setup_logging_with_filter};
use ruff_diagnostics::Applicability;
use ruff_source_file::{LineIndex, OneIndexed};
use std::backtrace::BacktraceStatus;
use std::fmt::{Display, Write};
use ty_module_resolver::{
    Module, SearchPath, SearchPathSettings, list_modules, resolve_module_confident,
};
use ty_python_semantic::pull_types::pull_types;
use ty_python_semantic::types::{UNDEFINED_REVEAL, check_types};
use ty_python_semantic::{
    MisconfigurationMode, Program, ProgramSettings, PythonEnvironment, PythonPlatform,
    PythonVersionSource, PythonVersionWithSource, SysPrefixPathOrigin,
};

mod assertion;
mod config;
mod db;
mod diagnostic;
mod external_dependencies;
mod matcher;
mod parser;

use ty_static::EnvVars;

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
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let suite = test_parser::parse(short_title, source)
        .map_err(|err| anyhow!("Failed to parse fixture: {err}"))?;

    let mut db = db::Db::setup();

    let filter = std::env::var(EnvVars::MDTEST_TEST_FILTER).ok();
    let mut any_failures = false;
    let mut assertion = String::new();
    for test in suite.tests() {
        if filter
            .as_ref()
            .is_some_and(|f| !(test.uncontracted_name().contains(f) || test.name() == *f))
        {
            continue;
        }

        let _tracing = test.configuration().log.as_ref().and_then(|log| match log {
            Log::Bool(enabled) => enabled.then(setup_logging),
            Log::Filter(filter) => setup_logging_with_filter(filter),
        });

        let result = run_test(
            &mut db,
            absolute_fixture_path,
            relative_fixture_path,
            snapshot_path,
            &test,
        );
        let inconsistencies = if result.as_ref().is_ok_and(|t| t.has_been_skipped()) {
            Ok(())
        } else {
            run_module_resolution_consistency_test(&db)
        };

        let this_test_failed = result.is_err() || inconsistencies.is_err();
        any_failures = any_failures || this_test_failed;

        if this_test_failed && output_format.is_cli() {
            let _ = writeln!(assertion, "\n\n{}\n", test.name().bold().underline());
        }

        if let Err(failures) = result {
            let md_index = LineIndex::from_source_text(source);

            for test_failures in failures {
                let source_map =
                    EmbeddedFileSourceMap::new(&md_index, test_failures.backtick_offsets);

                for (relative_line_number, failures) in test_failures.by_line.iter() {
                    let file = match output_format {
                        OutputFormat::Cli => relative_fixture_path.as_str(),
                        OutputFormat::GitHub => absolute_fixture_path.as_str(),
                    };

                    let absolute_line_number =
                        match source_map.to_absolute_line_number(relative_line_number) {
                            Ok(line_number) => line_number,
                            Err(last_line_number) => {
                                let _ = writeln!(
                                    assertion,
                                    "{}",
                                    output_format.display_error(
                                        file,
                                        last_line_number,
                                        "Found a trailing assertion comment \
                                        (e.g., `# revealed:` or `# error:`) \
                                        not followed by any statement."
                                    )
                                );

                                continue;
                            }
                        };

                    for failure in failures {
                        let _ = writeln!(
                            assertion,
                            "{}",
                            output_format.display_error(file, absolute_line_number, failure)
                        );
                    }
                }
            }
        }
        if let Err(inconsistencies) = inconsistencies {
            any_failures = true;
            for inconsistency in inconsistencies {
                match output_format {
                    OutputFormat::Cli => {
                        let info = relative_fixture_path.to_string().cyan();
                        let _ = writeln!(assertion, "  {info} {inconsistency}");
                    }
                    OutputFormat::GitHub => {
                        let _ = writeln!(
                            assertion,
                            "::error file={absolute_fixture_path}::{inconsistency}"
                        );
                    }
                }
            }
        }

        if this_test_failed && output_format.is_cli() {
            let escaped_test_name = test.name().replace('\'', "\\'");
            let _ = writeln!(
                assertion,
                "\nTo rerun this specific test, \
                set the environment variable: {}='{escaped_test_name}'",
                EnvVars::MDTEST_TEST_FILTER,
            );
            let _ = writeln!(
                assertion,
                "{}='{escaped_test_name}' cargo test -p ty_python_semantic \
                --test mdtest -- {test_name}",
                EnvVars::MDTEST_TEST_FILTER,
            );

            let _ = writeln!(assertion, "\n{}", "-".repeat(50));
        }
    }

    assert!(!any_failures, "{}", &assertion);

    Ok(())
}

/// Defines the format in which mdtest should print an error to the terminal
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// The format `cargo test` should use by default.
    Cli,
    /// A format that will provide annotations from GitHub Actions
    /// if mdtest fails on a PR.
    /// See <https://docs.github.com/en/actions/writing-workflows/choosing-what-your-workflow-does/workflow-commands-for-github-actions#setting-an-error-message>
    GitHub,
}

impl OutputFormat {
    const fn is_cli(self) -> bool {
        matches!(self, OutputFormat::Cli)
    }

    fn display_error(self, file: &str, line: OneIndexed, failure: impl Display) -> impl Display {
        struct Display<'a, T> {
            format: OutputFormat,
            file: &'a str,
            line: OneIndexed,
            failure: T,
        }

        impl<T> std::fmt::Display for Display<'_, T>
        where
            T: std::fmt::Display,
        {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let Display {
                    format,
                    file,
                    line,
                    failure,
                } = self;

                match format {
                    OutputFormat::Cli => {
                        write!(
                            f,
                            "  {file_line} {failure}",
                            file_line = format!("{file}:{line}").cyan()
                        )
                    }
                    OutputFormat::GitHub => {
                        write!(f, "::error file={file},line={line}::{failure}")
                    }
                }
            }
        }

        Display {
            format: self,
            file,
            line,
            failure,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TestOutcome {
    Success,
    Skipped,
}

impl TestOutcome {
    const fn has_been_skipped(self) -> bool {
        matches!(self, TestOutcome::Skipped)
    }
}

fn run_test(
    db: &mut db::Db,
    absolute_fixture_path: &Utf8Path,
    relative_fixture_path: &Utf8Path,
    snapshot_path: &Utf8Path,
    test: &parser::MarkdownTest,
) -> Result<TestOutcome, Failures> {
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
        if !std::env::var("MDTEST_EXTERNAL").is_ok_and(|v| v == "1") {
            return Ok(TestOutcome::Skipped);
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
                backtick_offsets: embedded.backtick_offsets.clone(),
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
            misconfiguration_mode: MisconfigurationMode::Fail,
        }
        .to_search_paths(db.system(), db.vendored())
        .expect("Failed to resolve search path settings"),
    };

    Program::init_or_update(db, settings);
    db.update_analysis_options(configuration.analysis.as_ref());

    // When snapshot testing is enabled, this is populated with
    // all diagnostics. Otherwise it remains empty.
    let mut snapshot_diagnostics = vec![];

    let mut any_pull_types_failures = false;
    let mut panic_info = None;

    let mut failures: Failures = test_files
        .iter()
        .filter_map(|test_file| {
            let parsed = parsed_module(db, test_file.file).load(db);

            let mut diagnostics: Vec<Diagnostic> = parsed
                .errors()
                .iter()
                .map(|error| Diagnostic::invalid_syntax(test_file.file, &error.error, error))
                .collect();

            diagnostics.extend(
                parsed
                    .unsupported_syntax_errors()
                    .iter()
                    .map(|error| Diagnostic::invalid_syntax(test_file.file, error, error)),
            );

            let mdtest_result = attempt_test(db, check_types, test_file);
            let type_diagnostics = match mdtest_result {
                Ok(diagnostics) => diagnostics,
                Err(failures) => {
                    if test.should_expect_panic().is_ok() {
                        panic_info = Some(failures.info);
                        return None;
                    }

                    return Some(failures.into_file_failures(db, "run mdtest", None));
                }
            };

            diagnostics.extend(type_diagnostics);
            diagnostics.sort_by(|left, right| {
                left.rendering_sort_key(db)
                    .cmp(&right.rendering_sort_key(db))
            });

            let failure = match matcher::match_file(db, test_file.file, &diagnostics) {
                Ok(()) => None,
                Err(line_failures) => Some(FileFailures {
                    backtick_offsets: test_file.backtick_offsets.clone(),
                    by_line: line_failures,
                }),
            };

            // Filter out `revealed-type` and `undefined-reveal` diagnostics from snapshots,
            // since they make snapshots very noisy!
            if test.should_snapshot_diagnostics() {
                snapshot_diagnostics.extend(diagnostics.into_iter().filter(|diagnostic| {
                    diagnostic.id() != DiagnosticId::RevealedType
                        && !diagnostic.id().is_lint_named(&UNDEFINED_REVEAL.name())
                }));
            }

            let pull_types_result = attempt_test(db, pull_types, test_file);
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

    match panic_info {
        Some(panic_info) => {
            let expected_message = test
                .should_expect_panic()
                .expect("panic_info is only set when `should_expect_panic` is `Ok`");

            let message = panic_info
                .payload
                .as_str()
                .unwrap_or("Box<dyn Any>")
                .to_string();

            if let Some(expected_message) = expected_message {
                assert!(
                    message.contains(expected_message),
                    "Test `{}` is expected to panic with `{expected_message}`, but panicked with `{message}` instead.",
                    test.name()
                );
            }
        }
        None => {
            if let Ok(message) = test.should_expect_panic() {
                if let Some(message) = message {
                    panic!(
                        "Test `{}` is expected to panic with `{message}`, but it didn't.",
                        test.name()
                    );
                }
                panic!("Test `{}` is expected to panic but it didn't.", test.name());
            }
        }
    }

    if test.should_skip_pulling_types() && !any_pull_types_failures {
        let mut by_line = matcher::FailuresByLine::default();
        by_line.push(
            OneIndexed::from_zero_indexed(0),
            vec![
                "Remove the `<!-- pull-types:skip -->` directive from this test: pulling types \
                 succeeded for all files in the test."
                    .to_string(),
            ],
        );
        let failure = FileFailures {
            backtick_offsets: test_files[0].backtick_offsets.clone(),
            by_line,
        };
        failures.push(failure);
    }

    if snapshot_diagnostics.is_empty() && test.should_snapshot_diagnostics() {
        panic!(
            "Test `{}` requested snapshotting diagnostics but it didn't produce any.",
            test.name()
        );
    } else if !snapshot_diagnostics.is_empty() {
        let snapshot =
            create_diagnostic_snapshot(db, relative_fixture_path, test, snapshot_diagnostics);
        let name = test.name().replace(' ', "_").replace(':', "__");
        insta::with_settings!(
            {
                snapshot_path => snapshot_path,
                input_file => name.clone(),
                filters => vec![(r"\\", "/")],
                prepend_module_to_snapshot => false,
            },
            { insta::assert_snapshot!(name, snapshot) }
        );
    }

    if failures.is_empty() {
        Ok(TestOutcome::Success)
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
                write!(f, " when listing modules, but `resolve_module` returned ",)?;
                fmt_module(self.db, f, got)?;
            }
        }
        Ok(())
    }
}

type Failures = Vec<FileFailures>;

/// The failures for a single file in a test by line number.
struct FileFailures {
    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    backtick_offsets: Vec<BacktickOffsets>,

    /// The failures by lines in the file.
    by_line: matcher::FailuresByLine,
}

/// File in a test.
struct TestFile {
    file: File,

    /// Positional information about the code block(s) to reconstruct absolute line numbers.
    backtick_offsets: Vec<BacktickOffsets>,
}

fn create_diagnostic_snapshot(
    db: &mut db::Db,
    relative_fixture_path: &Utf8Path,
    test: &parser::MarkdownTest,
    diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> String {
    let display_config = DisplayDiagnosticConfig::default()
        .color(false)
        .show_fix_diff(true)
        .with_fix_applicability(Applicability::DisplayOnly);

    let mut snapshot = String::new();
    writeln!(snapshot).unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot, "mdtest name: {}", test.uncontracted_name()).unwrap();
    writeln!(snapshot, "mdtest path: {relative_fixture_path}").unwrap();
    writeln!(snapshot, "---").unwrap();
    writeln!(snapshot).unwrap();

    writeln!(snapshot, "# Python source files").unwrap();
    writeln!(snapshot).unwrap();
    for file in test.files() {
        writeln!(snapshot, "## {}", file.relative_path()).unwrap();
        writeln!(snapshot).unwrap();
        // Note that we don't use ```py here because the line numbering
        // we add makes it invalid Python. This sacrifices syntax
        // highlighting when you look at the snapshot on GitHub,
        // but the line numbers are extremely useful for analyzing
        // snapshots. So we keep them.
        writeln!(snapshot, "```").unwrap();

        let line_number_width = file.code.lines().count().to_string().len();
        for (i, line) in file.code.lines().enumerate() {
            let line_number = i + 1;
            writeln!(snapshot, "{line_number:>line_number_width$} | {line}").unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        writeln!(snapshot).unwrap();
    }

    writeln!(snapshot, "# Diagnostics").unwrap();
    writeln!(snapshot).unwrap();
    for (i, diag) in diagnostics.into_iter().enumerate() {
        if i > 0 {
            writeln!(snapshot).unwrap();
        }
        writeln!(snapshot, "```").unwrap();
        write!(snapshot, "{}", diag.display(db, &display_config)).unwrap();
        writeln!(snapshot, "```").unwrap();
    }
    snapshot
}

/// Run a function over an embedded test file, catching any panics that occur in the process.
///
/// If no panic occurs, the result of the function is returned as an `Ok()` variant.
///
/// If a panic occurs, a nicely formatted [`FileFailures`] is returned as an `Err()` variant.
/// This will be formatted into a diagnostic message by `ty_test`.
fn attempt_test<'db, 'a, T, F>(
    db: &'db Db,
    test_fn: F,
    test_file: &'a TestFile,
) -> Result<T, AttemptTestError<'a>>
where
    F: FnOnce(&'db dyn ty_python_semantic::Db, File) -> T + std::panic::UnwindSafe,
{
    catch_unwind(|| test_fn(db, test_file.file))
        .map_err(|info| AttemptTestError { info, test_file })
}

struct AttemptTestError<'a> {
    info: PanicError,
    test_file: &'a TestFile,
}

impl AttemptTestError<'_> {
    fn into_file_failures(
        self,
        db: &Db,
        action: &str,
        clarification: Option<&str>,
    ) -> FileFailures {
        let info = self.info;

        let mut by_line = matcher::FailuresByLine::default();
        let mut messages = vec![];
        match info.location {
            Some(location) => messages.push(format!(
                "Attempting to {action} caused a panic at {location}"
            )),
            None => messages.push(format!(
                "Attempting to {action} caused a panic at an unknown location",
            )),
        }
        if let Some(clarification) = clarification {
            messages.push(clarification.to_string());
        }
        messages.push(String::new());
        match info.payload.as_str() {
            Some(message) => messages.push(message.to_string()),
            // Mimic the default panic hook's rendering of the panic payload if it's
            // not a string.
            None => messages.push("Box<dyn Any>".to_string()),
        }
        messages.push(String::new());

        if let Some(backtrace) = info.backtrace {
            match backtrace.status() {
                BacktraceStatus::Disabled => {
                    let msg =
                        "run with `RUST_BACKTRACE=1` environment variable to display a backtrace";
                    messages.push(msg.to_string());
                }
                BacktraceStatus::Captured => {
                    messages.extend(backtrace.to_string().split('\n').map(String::from));
                }
                _ => {}
            }
        }

        if let Some(backtrace) = info.salsa_backtrace {
            salsa::attach(db, || {
                messages.extend(format!("{backtrace:#}").split('\n').map(String::from));
            });
        }

        by_line.push(OneIndexed::from_zero_indexed(0), messages);

        FileFailures {
            backtick_offsets: self.test_file.backtick_offsets.clone(),
            by_line,
        }
    }
}
