#![allow(clippy::disallowed_names)]
use ruff_benchmark::criterion;

use std::ops::Range;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use rayon::ThreadPoolBuilder;
use rustc_hash::FxHashSet;

use ruff_benchmark::TestFile;
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::{File, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{MemoryFileSystem, SystemPath, SystemPathBuf, TestSystem};
use ruff_python_ast::PythonVersion;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::value::RangedValue;
use ty_project::watch::{ChangeEvent, ChangedKind};
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

struct Case {
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

fn setup_tomllib_case() -> Case {
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
    metadata.apply_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(PythonVersion::PY312)),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let mut db = ProjectDatabase::new(metadata, system).unwrap();
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

    db.project().set_open_files(&mut db, tomllib_files);

    let re_path = re.path(&db).as_system_path().unwrap().to_owned();
    Case {
        db,
        fs,
        file: re,
        file_path: re_path,
    }
}

static RAYON_INITIALIZED: std::sync::Once = std::sync::Once::new();

fn setup_rayon() {
    // Initialize the rayon thread pool outside the benchmark because it has a significant cost.
    // We limit the thread pool to only one (the current thread) because we're focused on
    // where ty spends time and less about how well the code runs concurrently.
    // We might want to add a benchmark focusing on concurrency to detect congestion in the future.
    RAYON_INITIALIZED.call_once(|| {
        ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
            .unwrap();
    });
}

fn benchmark_incremental(criterion: &mut Criterion) {
    fn setup() -> Case {
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

    fn incremental(case: &mut Case) {
        let Case { db, .. } = case;

        db.apply_changes(
            vec![ChangeEvent::Changed {
                path: case.file_path.clone(),
                kind: ChangedKind::FileContent,
            }],
            None,
        );

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
                let Case { db, .. } = case;
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
                diagnostic.primary_message(),
                diagnostic.severity(),
            )
        })
        .collect();
    assert_eq!(&normalized, expected);
}

fn setup_micro_case(code: &str) -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();

    let file_path = "src/test.py";
    fs.write_file_all(
        SystemPathBuf::from(file_path),
        ruff_python_trivia::textwrap::dedent(code),
    )
    .unwrap();

    let src_root = SystemPath::new("/src");
    let mut metadata = ProjectMetadata::discover(src_root, &system).unwrap();
    metadata.apply_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(PythonVersion::PY312)),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let mut db = ProjectDatabase::new(metadata, system).unwrap();
    let file = system_path_to_file(&db, SystemPathBuf::from(file_path)).unwrap();

    db.project()
        .set_open_files(&mut db, FxHashSet::from_iter([file]));

    let file_path = file.path(&db).as_system_path().unwrap().to_owned();

    Case {
        db,
        fs,
        file,
        file_path,
    }
}

fn benchmark_many_string_assignments(criterion: &mut Criterion) {
    setup_rayon();

    criterion.bench_function("ty_micro[many_string_assignments]", |b| {
        b.iter_batched_ref(
            || {
                // This is a micro benchmark, but it is effectively identical to a code sample
                // observed "in the wild":
                setup_micro_case(
                    r#"
                    def f(x) -> str:
                        s = ""
                        # Each conditional doubles the size of the union of string literal types,
                        # so if we go up to attr10, we have 2**10 = 1024 string literal types
                        if x.attr1:
                            s += "attr1"
                        if x.attr2:
                            s += "attr2"
                        if x.attr3:
                            s += "attr3"
                        if x.attr4:
                            s += "attr4"
                        if x.attr5:
                            s += "attr5"
                        if x.attr6:
                            s += "attr6"
                        if x.attr7:
                            s += "attr7"
                        if x.attr8:
                            s += "attr8"
                        if x.attr9:
                            s += "attr9"
                        if x.attr10:
                            s += "attr10"
                        # The above checked how fast we are in building the union; this checks how
                        # we manage it once it is built. If implemented naively, this has to check
                        # each member of the union for compatibility with the Sized protocol.
                        if len(s) > 0:
                            s = s[:-3]
                        return s
                    "#,
                )
            },
            |case| {
                let Case { db, .. } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_many_tuple_assignments(criterion: &mut Criterion) {
    setup_rayon();

    criterion.bench_function("ty_micro[many_tuple_assignments]", |b| {
        b.iter_batched_ref(
            || {
                // This is a micro benchmark, but it is effectively identical to a code sample
                // observed in https://github.com/astral-sh/ty/issues/362
                setup_micro_case(
                    r#"
                    def flag() -> bool:
                        return True

                    t = ()
                    if flag():
                        t += (1,)
                    if flag():
                        t += (2,)
                    if flag():
                        t += (3,)
                    if flag():
                        t += (4,)
                    if flag():
                        t += (5,)
                    if flag():
                        t += (6,)
                    if flag():
                        t += (7,)
                    if flag():
                        t += (8,)

                    # Perform some kind of operation on the union type
                    print(1 in t)
                    "#,
                )
            },
            |case| {
                let Case { db, .. } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(check_file, benchmark_cold, benchmark_incremental);
criterion_group!(
    micro,
    benchmark_many_string_assignments,
    benchmark_many_tuple_assignments,
);
criterion_main!(check_file, micro);
