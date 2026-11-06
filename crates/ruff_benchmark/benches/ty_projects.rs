use ruff_benchmark::criterion;
use ruff_benchmark::real_world_projects::{
    InstalledProject, RealWorldProject, TY_ECOSYSTEM_PIN, check_project,
};

use std::ops::Range;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use rustc_hash::FxHashSet;

use ruff_benchmark::TestFile;
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{InMemorySystem, MemoryFileSystem, SystemPath, SystemPathBuf, TestSystem};
use ruff_ranged_value::RangedValue;
use ty_project::metadata::options::{AnalysisOptions, EnvironmentOptions, Options, Rules};
use ty_project::metadata::python_version::SupportedPythonVersion;
use ty_project::metadata::value::RelativePathBuf;
use ty_project::watch::{ChangeEvent, ChangedKind};
use ty_project::{CheckMode, Db, ProjectDatabase, ProjectMetadata};
use ty_python_semantic::lint::Level;

#[path = "ty_shared/rayon.rs"]
mod rayon;

use self::rayon::setup_rayon;

struct FileCase {
    db: ProjectDatabase,
    fs: MemoryFileSystem,
    file: File,
    file_path: SystemPathBuf,
}

// "https://raw.githubusercontent.com/python/cpython/8e8a4baf652f6e1cee7acde9d78c4b6154539748/Lib/tomllib";
static TOMLLIB_FILES: [TestFile; 4] = [
    TestFile::new(
        "tomllib/__init__.py",
        include_str!("../resources/tomllib/__init__.py"),
    ),
    TestFile::new(
        "tomllib/_parser.py",
        include_str!("../resources/tomllib/_parser.py"),
    ),
    TestFile::new(
        "tomllib/_re.py",
        include_str!("../resources/tomllib/_re.py"),
    ),
    TestFile::new(
        "tomllib/_types.py",
        include_str!("../resources/tomllib/_types.py"),
    ),
];

/// A structured set of fields we use to do diagnostic comparisons.
///
/// This helps assert benchmark results. Previously, we would compare
/// the actual diagnostic output, but using `insta` inside benchmarks is
/// problematic, and updating the strings otherwise when diagnostic rendering
/// changes is a PITA.
type KeyDiagnosticFields = (
    DiagnosticId,
    Option<&'static str>,
    Option<Range<usize>>,
    &'static str,
    Severity,
);

static EXPECTED_TOMLLIB_DIAGNOSTICS: &[KeyDiagnosticFields] = &[];

fn tomllib_path(file: &TestFile) -> SystemPathBuf {
    SystemPathBuf::from("src").join(file.name())
}

fn setup_tomllib_case() -> FileCase {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();

    fs.write_files_all(
        TOMLLIB_FILES
            .iter()
            .map(|file| (tomllib_path(file), file.code().to_string())),
    )
    .unwrap();

    let src_root = SystemPath::new("/src");
    let mut metadata = ProjectMetadata::discover(src_root, &system).unwrap();
    metadata.set_override_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(SupportedPythonVersion::Py312)),
            ..EnvironmentOptions::default()
        }),
        analysis: Some(AnalysisOptions {
            respect_type_ignore_comments: Some(false),
            ..AnalysisOptions::default()
        }),
        ..Options::default()
    });

    let mut db = ProjectDatabase::fallible(metadata, system).unwrap();
    let mut tomllib_files = FxHashSet::default();
    let mut re: Option<File> = None;

    for test_file in &TOMLLIB_FILES {
        let file = system_path_to_file(&db, tomllib_path(test_file)).unwrap();
        if test_file.name().ends_with("_re.py") {
            re = Some(file);
        }
        tomllib_files.insert(file);
    }

    let re = re.unwrap();

    db.set_check_mode(CheckMode::OpenFiles);
    db.project().set_open_files(&mut db, tomllib_files);

    let re_path = re.path(&db).as_system_path().unwrap().to_owned();
    FileCase {
        db,
        fs,
        file: re,
        file_path: re_path,
    }
}

fn benchmark_incremental(criterion: &mut Criterion) {
    fn setup() -> FileCase {
        let case = setup_tomllib_case();

        let result: Vec<_> = case.db.check();

        assert_diagnostics(&case.db, &result, EXPECTED_TOMLLIB_DIAGNOSTICS);

        case.fs
            .write_file_all(
                &case.file_path,
                format!(
                    "{}\n# A comment\n",
                    source_text(&case.db, case.file).as_str()
                ),
            )
            .unwrap();

        case
    }

    fn incremental(case: &mut FileCase) {
        let FileCase { db, .. } = case;

        db.apply_changes(&[ChangeEvent::Changed {
            path: case.file_path.clone(),
            kind: ChangedKind::FileContent,
        }]);

        let result = db.check();

        assert_eq!(result.len(), EXPECTED_TOMLLIB_DIAGNOSTICS.len());
    }

    setup_rayon();

    criterion.bench_function("ty_check_file[incremental]", |b| {
        b.iter_batched_ref(setup, incremental, BatchSize::SmallInput);
    });
}

fn benchmark_cold(criterion: &mut Criterion) {
    setup_rayon();

    criterion.bench_function("ty_check_file[cold]", |b| {
        b.iter_batched_ref(
            setup_tomllib_case,
            |case| {
                let FileCase { db, .. } = case;
                let result: Vec<_> = db.check();

                assert_diagnostics(db, &result, EXPECTED_TOMLLIB_DIAGNOSTICS);
            },
            BatchSize::SmallInput,
        );
    });
}

#[track_caller]
fn assert_diagnostics(db: &dyn Db, diagnostics: &[Diagnostic], expected: &[KeyDiagnosticFields]) {
    let normalized: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| {
            (
                diagnostic.id(),
                diagnostic
                    .primary_span()
                    .map(|span| span.expect_ty_file())
                    .map(|file| file.path(db).as_str()),
                diagnostic
                    .primary_span()
                    .and_then(|span| span.range())
                    .map(Range::<usize>::from),
                diagnostic.headline_message(),
                diagnostic.severity(),
            )
        })
        .collect();
    assert_eq!(&normalized, expected);
}

struct ProjectBenchmark<'a> {
    project: InstalledProject<'a>,
    fs: MemoryFileSystem,
    freeze_inputs: bool,
    rules: Option<Rules>,
}

impl<'a> ProjectBenchmark<'a> {
    fn new(project: RealWorldProject<'a>) -> Self {
        let setup_project = project.setup().expect("Failed to setup project");
        let fs = setup_project
            .copy_to_memory_fs()
            .expect("Failed to copy project to memory fs");

        Self {
            project: setup_project,
            fs,
            freeze_inputs: false,
            rules: None,
        }
    }

    fn freeze_inputs(mut self) -> Self {
        self.freeze_inputs = true;
        self
    }

    fn setup_iteration(&self) -> ProjectDatabase {
        let system = TestSystem::new(InMemorySystem::from_memory_fs(self.fs.clone()));

        let src_root = SystemPath::new("/");
        let mut metadata = ProjectMetadata::discover(src_root, &system).unwrap();

        metadata.set_override_options(Options {
            environment: Some(EnvironmentOptions {
                python_version: Some(RangedValue::cli(self.project.config.python_version)),
                python: Some(RelativePathBuf::cli(SystemPath::new(".venv"))),
                ..EnvironmentOptions::default()
            }),
            rules: self.rules.clone(),
            ..Options::default()
        });

        let mut db = ProjectDatabase::fallible(metadata, system).unwrap();

        db.project().set_included_paths(
            &mut db,
            self.project
                .check_paths()
                .iter()
                .map(|path| SystemPathBuf::from(*path))
                .collect(),
        );

        if self.freeze_inputs {
            db.freeze();
        }

        db
    }
}

#[track_caller]
fn bench_project(benchmark: &ProjectBenchmark, criterion: &mut Criterion) {
    bench_project_named(benchmark, criterion, benchmark.project.config.name);
}

#[track_caller]
fn bench_project_named(
    benchmark: &ProjectBenchmark,
    criterion: &mut Criterion,
    benchmark_name: &str,
) {
    setup_rayon();

    let mut group = criterion.benchmark_group("project");
    group.sampling_mode(criterion::SamplingMode::Flat);
    group.bench_function(benchmark_name, |b| {
        b.iter_batched_ref(
            || benchmark.setup_iteration(),
            |db| check_project(db, benchmark_name),
            BatchSize::SmallInput,
        );
    });
}

fn hydra(criterion: &mut Criterion) {
    let benchmark = ProjectBenchmark::new(RealWorldProject {
        name: "hydra-zen",
        repository: "https://github.com/mit-ll-responsible-ai/hydra-zen",
        commit: "03a01096ea6a7c574fdf0b9990056506e566df2d",
        paths: &["src", "tests/annotations"],
        dependencies: &["pydantic", "beartype", "hydra-core"],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    });

    bench_project(&benchmark, criterion);
}

fn attrs(criterion: &mut Criterion) {
    let benchmark = ProjectBenchmark::new(RealWorldProject {
        name: "attrs",
        repository: "https://github.com/python-attrs/attrs",
        commit: "89fae8300f484544c1b7678cea5efe58c551fbb9",
        paths: &["src/attrs", "src/attr", "typing-examples"],
        dependencies: &[],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    });

    bench_project(&benchmark, criterion);

    // Keep one real-world benchmark frozen to catch regressions from newly added inputs.
    let frozen_benchmark = benchmark.freeze_inputs();
    bench_project_named(&frozen_benchmark, criterion, "attrs (frozen inputs)");

    let all_rules_benchmark = ProjectBenchmark {
        freeze_inputs: false,
        rules: Some(Rules::from_iter([(
            RangedValue::cli("all".to_owned()),
            RangedValue::cli(Level::Error),
        )])),
        ..frozen_benchmark
    };

    bench_project_named(&all_rules_benchmark, criterion, "attrs (all rules)");
}

fn anyio(criterion: &mut Criterion) {
    let benchmark = ProjectBenchmark::new(RealWorldProject {
        name: "anyio",
        repository: "https://github.com/agronholm/anyio",
        commit: "ffe91331adb912c5d150f5d373f7cd28a0e96a62",
        paths: &["src"],
        dependencies: &["exceptiongroup", "idna", "pytest"],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    });

    bench_project(&benchmark, criterion);
}

fn datetype(criterion: &mut Criterion) {
    let benchmark = ProjectBenchmark::new(RealWorldProject {
        name: "DateType",
        repository: "https://github.com/glyph/DateType",
        commit: "a6ebb954cd18302a031a29b2f65e077b8e7776d4",
        paths: &["src"],
        dependencies: &[],
        max_dep_date: TY_ECOSYSTEM_PIN,
        python_version: SupportedPythonVersion::Py311,
    });

    bench_project(&benchmark, criterion);
}

criterion_group!(check_file, benchmark_cold, benchmark_incremental);
criterion_group!(project, anyio, attrs, hydra, datetype);
criterion_main!(check_file, project);
