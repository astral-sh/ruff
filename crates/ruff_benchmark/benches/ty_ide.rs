use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::hint::black_box;

use divan::Bencher;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::system::{DbWithWritableSystem as _, OsSystem, SystemPath, SystemPathBuf, TestSystem};
use ruff_ranged_value::RangedValue;
use ruff_text_size::{Ranged, TextRange, TextSize};
use ty_ide::{Completion, CompletionCapabilities, CompletionSettings};
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::python_version::SupportedPythonVersion;
use ty_project::metadata::value::RelativePathBuf;
use ty_project::{ProjectDatabase, ProjectMetadata, SemanticDb as _};

mod auto_import {
    //! Auto-import benchmarks using `ty_ide::completion`.
    //!
    //! The timer covers the API call and disposal of its results, without an LSP server.
    //! Each input owns a fresh project and database, with warm filesystem caches.
    //! Fixture generation, warm-up requests, edits, database notifications, validation,
    //! and project/database teardown are outside the timed region.

    use super::*;

    /// Number of leaf modules generated for each fixture.
    const LEAVES: usize = 1_000;

    /// Completion prefix used by [`Fixture`] to match ten instances of a symbol of
    /// the form `BenchmarkNeedleXXX`.
    ///
    /// Matching ten symbols helps limit import-edit
    /// and ranking work that is irrelevant to testing auto-imports, and the textual
    /// prefix avoids accidental matches from subsequence matching of digits.
    const QUERY: &str = "BenchmarkNeedle";

    /// Class name introduced by the module-creation case.
    const ADDED_SYMBOL: &str = "BenchmarkNeedleAdded";

    /// Python source for each leaf, with placeholders for its class name and module index.
    const LEAF_SOURCE: &str = r#"
class {symbol}:
    value: int = {index}


def benchmark_function_{index:03}(value: int) -> int:
    return value + {index}


BENCHMARK_CONSTANT_{index:03} = {index}
"#;

    /// Directory layouts for [`Fixture`].
    #[derive(Clone, Copy)]
    enum Layout {
        /// Regular packages nested six levels beneath one search root.
        /// The 1,000 leaf modules form 40 groups of 25: `group_00` contains modules 000–024,
        /// through to `group_39` containing modules 975–999. Every package directory
        /// contains `__init__.py`, omitted below:
        ///
        /// ```text
        /// roots/root_00/benchpkg/
        /// ├── group_00/level_0/level_1/level_2/level_3/
        /// │   ├── module_000.py
        /// │   ├── ...
        /// │   └── module_024.py
        /// ├── ...
        /// └── group_39/level_0/level_1/level_2/level_3/
        ///     ├── module_975.py
        ///     ├── ...
        ///     └── module_999.py
        /// ```
        RegularDeep,
        /// Namespace packages nested two levels beneath 16 search roots, without initializers.
        /// The groups cover the same batches of 25 modules as [`Layout::RegularDeep`], but each
        /// module goes in root `index % 16`. Thus each group spans all 16 roots, and each root
        /// contains one or two modules per group. Selected groups in the first and last roots:
        ///
        /// ```text
        /// roots/
        /// ├── root_00/benchpkg/
        /// │   ├── group_00/
        /// │   │   ├── module_000.py
        /// │   │   └── module_016.py
        /// │   ├── ...
        /// │   └── group_39/
        /// │       ├── module_976.py
        /// │       └── module_992.py
        /// ├── ...
        /// └── root_15/benchpkg/
        ///     ├── group_01/
        ///     │   ├── module_031.py
        ///     │   └── module_047.py
        ///     ├── ...
        ///     └── group_39/
        ///         ├── module_975.py
        ///         └── module_991.py
        /// ```
        NamespaceSplit,
    }

    impl Layout {
        /// Returns the package depth and the number of search roots containing packages.
        fn parameters(self) -> LayoutParameters {
            match self {
                Self::RegularDeep => LayoutParameters {
                    package_depth: 6,
                    root_count: 1,
                },
                Self::NamespaceSplit => LayoutParameters {
                    package_depth: 2,
                    root_count: 16,
                },
            }
        }
    }

    impl fmt::Display for Layout {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(match self {
                Self::RegularDeep => "regular-package",
                Self::NamespaceSplit => "namespace-package",
            })
        }
    }

    /// Dimensions used to construct a [`Layout`].
    struct LayoutParameters {
        /// Number of package directories between a search root and a leaf module.
        package_depth: usize,
        /// Number of search roots across which leaf modules are distributed, excluding `app`.
        root_count: usize,
    }

    /// A temporary project whose leaf modules are grouped in batches of 25 according to [`Layout`].
    /// `app` and each `roots/root_NN` are import search paths.
    ///
    /// Each leaf defines a class, function, and constant using [`LEAF_SOURCE`]. Its
    /// `{symbol}` placeholder becomes `BenchmarkItem000` through `BenchmarkItem989` for
    /// the first 990 modules, and `BenchmarkNeedle990` through `BenchmarkNeedle999` for
    /// the final ten.
    ///
    /// `app/main.py` contains only [`QUERY`] (`BenchmarkNeedle`), with the completion cursor
    /// immediately after it. No imports are present, so the ten matching classes require
    /// auto-imports.
    struct Fixture {
        project_root: SystemPathBuf,
        search_paths: Vec<SystemPathBuf>,
        modules: BTreeMap<String, Option<SystemPathBuf>>,
        symbols: BTreeMap<String, String>,
        cursor: TextSize,
        target: String,
        target_directory: SystemPathBuf,
        // Keep files alive through the database's lifetime, then remove this input completely.
        _directory: tempfile::TempDir,
    }

    impl Fixture {
        /// Creates the selected layout and records its expected modules and matching symbols.
        fn new(layout: Layout) -> Self {
            let directory = tempfile::tempdir().expect("Create fixture directory");
            let project_root = SystemPathBuf::from_path_buf(
                directory
                    .path()
                    .canonicalize()
                    .expect("Canonicalize fixture"),
            )
            .expect("UTF-8 fixture path");
            let LayoutParameters {
                package_depth,
                root_count,
            } = layout.parameters();
            let mut search_paths = vec![project_root.join("app")];
            search_paths.extend(
                (0..root_count).map(|index| project_root.join(format!("roots/root_{index:02}"))),
            );
            for path in &search_paths {
                std::fs::create_dir_all(path).expect("Create search root");
            }
            write_source(
                &project_root.join("app/main.py"),
                &format!(
                    r#"{QUERY}
"#,
                ),
            );
            let mut modules = BTreeMap::new();
            let mut symbols = BTreeMap::new();
            let mut target = String::new();
            let mut target_directory = project_root.clone();
            for index in 0..LEAVES {
                let root_index = index % root_count;
                let mut path = search_paths[root_index + 1].clone();
                let mut components =
                    vec!["benchpkg".to_owned(), format!("group_{:02}", index / 25)];
                components.extend((0..package_depth - 2).map(|level| format!("level_{level}")));
                let mut name = String::new();
                for component in components {
                    path.push(&component);
                    if !name.is_empty() {
                        name.push('.');
                    }
                    name.push_str(&component);
                    std::fs::create_dir_all(&path).expect("Create package directory");
                    let initializer = if matches!(layout, Layout::NamespaceSplit) {
                        None
                    } else {
                        let initializer = path.join("__init__.py");
                        write_source(
                            &initializer,
                            r#""""An ordinary package in the auto-import benchmark."""
"#,
                        );
                        Some(initializer)
                    };
                    modules.insert(name.clone(), initializer);
                }
                let filename = format!("module_{index:03}.py");
                write!(name, ".module_{index:03}").expect("Append module name");
                let symbol = if index >= LEAVES - 10 {
                    let symbol = format!("{QUERY}{index:03}");
                    symbols.insert(symbol.clone(), name.clone());
                    symbol
                } else {
                    format!("BenchmarkItem{index:03}")
                };
                let source = LEAF_SOURCE
                    .trim_start()
                    .replace("{symbol}", &symbol)
                    .replace("{index:03}", &format!("{index:03}"))
                    .replace("{index}", &index.to_string());
                if index == LEAVES - 1 {
                    target.clone_from(&name);
                    target_directory.clone_from(&path);
                }
                let module_path = path.join(filename);
                write_source(&module_path, &source);
                modules.insert(name, Some(module_path));
            }
            Self {
                project_root,
                search_paths,
                modules,
                symbols,
                cursor: QUERY.len().try_into().expect("Cursor fits in TextSize"),
                target,
                target_directory,
                _directory: directory,
            }
        }

        /// Returns the dotted name of `benchmark_added`, a sibling of the final leaf module.
        fn added_name(&self) -> String {
            let (parent, _) = self.target.rsplit_once('.').expect("Target has a package");
            format!("{parent}.benchmark_added")
        }

        /// Returns the path of `benchmark_added.py`, beside the final leaf module.
        fn added_path(&self) -> SystemPathBuf {
            self.target_directory.join("benchmark_added.py")
        }
    }

    /// Selects the preparation performed before the measured completion request.
    #[derive(Clone, Copy)]
    enum Mode {
        /// A fresh database with no previous completion request.
        FirstRequest,
        /// Repeat the same request without changing files.
        RepeatedRequest,
        /// Warm a request, then add a sibling of the last leaf with another matching class.
        AfterModuleCreate,
    }

    impl fmt::Display for Mode {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(match self {
                Self::FirstRequest => "first-request",
                Self::RepeatedRequest => "repeated-request",
                Self::AfterModuleCreate => "after-module-create",
            })
        }
    }

    const MODES: [Mode; 3] = [
        Mode::FirstRequest,
        Mode::RepeatedRequest,
        Mode::AfterModuleCreate,
    ];

    /// A layout and benchmark mode, displayed together in benchmark reports.
    struct Scenario {
        layout: Layout,
        mode: Mode,
    }

    impl fmt::Display for Scenario {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}/{}", self.layout, self.mode)
        }
    }

    /// A fixture and database for one measured request.
    struct Case {
        db: ProjectDatabase,
        client: File,
        fixture: Fixture,
        added: bool,
    }

    impl Case {
        /// Creates a fresh fixture and database without making a completion request.
        fn new(layout: Layout) -> Self {
            let fixture = Fixture::new(layout);
            let system = TestSystem::new(OsSystem::new(&fixture.project_root));
            system.clear_env_vars();
            // Synthetic cases must not inherit configuration from the temporary directory's ancestors.
            let mut metadata =
                ProjectMetadata::new("auto-import-benchmark", fixture.project_root.clone());
            metadata.apply_override_options(Options {
                environment: Some(EnvironmentOptions {
                    root: Some(
                        fixture
                            .search_paths
                            .iter()
                            .map(RelativePathBuf::cli)
                            .collect(),
                    ),
                    python_version: Some(RangedValue::cli(SupportedPythonVersion::Py312)),
                    ..EnvironmentOptions::default()
                }),
                ..Options::default()
            });
            let db =
                ProjectDatabase::fallible(metadata, system).expect("Create benchmark database");
            let client = system_path_to_file(&db, fixture.project_root.join("app/main.py"))
                .expect("Find client file");
            // Initialize the resolver environment without warming completion queries.
            let _ = db.program_file(client).resolver_environment(&db);
            Self {
                db,
                client,
                fixture,
                added: false,
            }
        }

        /// Prepares a fresh case for the selected mode. Call once before measuring a request.
        fn prepare(&mut self, mode: Mode) {
            match mode {
                Mode::FirstRequest => {}
                Mode::RepeatedRequest => drop(self.completions()),
                Mode::AfterModuleCreate => {
                    drop(self.completions());
                    let path = self.fixture.added_path();
                    self.db
                        .write_dedented(
                            path.as_str(),
                            &format!(
                                r#"
                                class {ADDED_SYMBOL}:
                                    pass
                                "#,
                            ),
                        )
                        .expect("Write added module");
                    self.added = true;
                }
            }
        }

        /// Requests completions after `QUERY` in the client file, using default settings.
        fn completions(&self) -> Vec<Completion<'_>> {
            ty_ide::completion(
                &self.db,
                &CompletionSettings::default(),
                CompletionCapabilities::default(),
                self.db.program_file(self.client),
                self.fixture.cursor,
            )
        }

        /// Asserts that suggestions and enumerated modules match the fixture's expectations,
        /// so that we can ensure that the benchmark scenario is accurate.
        ///
        /// For instance, a bug that caused us to skip enumeration of the `group_00` package
        /// (see [`Layout`]) would leave all matching classes (in `group_39`) available, so
        /// completion assertions alone would pass (and it might appear to improve performance).
        ///
        /// To prevent that, this method runs some basic validations of the output of our
        /// completions and module enumeration interfaces.
        fn validate(&self) {
            self.validate_completions();

            let db = &self.db;
            let file = db.program_file(self.client);
            let modules = ty_module_resolver::all_modules(db, file.resolver_environment(db));
            let mut actual = BTreeMap::new();
            for module in modules {
                let name = module.name(db).as_str();
                if name == "benchpkg" || name.starts_with("benchpkg.") {
                    let origin = module.file(db).map(|file| {
                        file.path(db)
                            .as_system_path()
                            .expect("Fixture file is on disk")
                            .to_path_buf()
                    });
                    assert!(
                        actual.insert(name.to_owned(), origin).is_none(),
                        "Duplicate module {name}"
                    );
                }
            }
            let mut expected = self.fixture.modules.clone();
            if self.added {
                expected.insert(self.fixture.added_name(), Some(self.fixture.added_path()));
            }
            assert_eq!(actual.len(), expected.len(), "Fixture module count differs");
            assert_eq!(actual, expected, "Fixture modules or origins differ");
        }

        /// Validates that completions for each symbol match the expectations of the fixture.
        ///
        /// For instance, a bug could cause us to consult a stale cache after module creation
        /// and hence return the original ten suggestions quickly (an apparent performance
        /// improvement) but incorrectly omit `BenchmarkNeedleAdded` (see [`ADDED_SYMBOL`]).
        ///
        /// To prevent that this, validates that the results of completion are those that we expect.
        fn validate_completions(&self) {
            let mut expected = self.fixture.symbols.clone();
            if self.added {
                expected.insert(ADDED_SYMBOL.to_owned(), self.fixture.added_name());
            }

            let mut actual = BTreeMap::new();
            for completion in self.completions() {
                let name = completion.name.to_string();
                let module = completion
                    .module_name
                    .expect("Auto-import completion has an originating module");
                let edit = completion
                    .import
                    .expect("Auto-import completion inserts an import");
                assert_eq!(edit.range(), TextRange::empty(TextSize::ZERO));
                assert_eq!(
                    edit.content().expect("Import edit has text").trim(),
                    format!("from {module} import {name}"),
                    "Import edit differs for {name}"
                );
                assert!(
                    actual.insert(name.clone(), module.to_string()).is_none(),
                    "Duplicate completion {name}"
                );
            }
            assert_eq!(actual, expected, "Fixture completions or origins differ");
        }
    }

    fn benchmark(bencher: Bencher, layout: Layout, mode: Mode) {
        let make_case = || {
            let mut case = Case::new(layout);
            case.prepare(mode);
            case
        };

        // Validate names and origins on a separate input so these checks neither warm the
        // measured database nor add work to the timed completion request.
        let validation = make_case();
        validation.validate();
        drop(validation);

        bencher
            .with_inputs(make_case)
            .bench_local_refs(|case| black_box(case.completions()).len());
    }

    fn write_source(path: &SystemPath, source: &str) {
        std::fs::write(path, source).expect("Write fixture file");
    }

    #[divan::bench(
        name = "auto_imports",
        args = MODES.map(|mode| Scenario { layout: Layout::RegularDeep, mode }),
        sample_size = 1,
        sample_count = 10
    )]
    fn regular(bencher: Bencher, scenario: &Scenario) {
        benchmark(bencher, scenario.layout, scenario.mode);
    }

    #[divan::bench(
        name = "auto_imports",
        args = MODES.map(|mode| Scenario { layout: Layout::NamespaceSplit, mode }),
        ignore = true, // TODO: Remove this after namespace enumeration lands: https://github.com/astral-sh/ty/issues/2273
        sample_size = 1,
        sample_count = 10
    )]
    fn namespace(bencher: Bencher, scenario: &Scenario) {
        benchmark(bencher, scenario.layout, scenario.mode);
    }
}

fn main() {
    // Run symbol discovery on the current thread to reduce benchmark noise.
    rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .expect("Initialize benchmark worker pool");
    divan::main();
}
