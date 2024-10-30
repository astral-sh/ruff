#![allow(clippy::disallowed_names)]

use rayon::ThreadPoolBuilder;
use red_knot_python_semantic::PythonVersion;
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::watch::{ChangeEvent, ChangedKind};
use red_knot_workspace::workspace::settings::Configuration;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_benchmark::criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ruff_benchmark::TestFile;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::source::source_text;
use ruff_db::system::{MemoryFileSystem, SystemPath, SystemPathBuf, TestSystem};
use rustc_hash::FxHashSet;

struct Case {
    db: RootDatabase,
    fs: MemoryFileSystem,
    re: File,
    re_path: SystemPathBuf,
}

const TOMLLIB_312_URL: &str = "https://raw.githubusercontent.com/python/cpython/8e8a4baf652f6e1cee7acde9d78c4b6154539748/Lib/tomllib";

static EXPECTED_DIAGNOSTICS: &[&str] = &[
    // We don't support `*` imports yet:
    "/src/tomllib/_parser.py:7:29: Module `collections.abc` has no member `Iterable`",
    // We don't support terminal statements in control flow yet:
    "/src/tomllib/_parser.py:66:18: Name `s` used when possibly not defined",
    "/src/tomllib/_parser.py:98:12: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:101:12: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:104:14: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:108:17: Conflicting declared types for `second_char`: Unknown, str | None",
    "/src/tomllib/_parser.py:115:14: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:126:12: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:267:9: Conflicting declared types for `char`: Unknown, str | None",
    "/src/tomllib/_parser.py:348:20: Name `nest` used when possibly not defined",
    "/src/tomllib/_parser.py:353:5: Name `nest` used when possibly not defined",
    "/src/tomllib/_parser.py:353:5: Method `__getitem__` of type `Unbound | @Todo` is not callable on object of type `Unbound | @Todo`",
    "/src/tomllib/_parser.py:364:9: Conflicting declared types for `char`: Unknown, str | None",
    "/src/tomllib/_parser.py:381:13: Conflicting declared types for `char`: Unknown, str | None",
    "/src/tomllib/_parser.py:395:9: Conflicting declared types for `char`: Unknown, str | None",
    "/src/tomllib/_parser.py:453:24: Name `nest` used when possibly not defined",
    "/src/tomllib/_parser.py:455:9: Name `nest` used when possibly not defined",
    "/src/tomllib/_parser.py:455:9: Method `__getitem__` of type `Unbound | @Todo` is not callable on object of type `Unbound | @Todo`",
    "/src/tomllib/_parser.py:482:16: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:566:12: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:573:12: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:579:12: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:580:63: Name `char` used when possibly not defined",
    "/src/tomllib/_parser.py:590:9: Conflicting declared types for `char`: Unknown, str | None",
    "/src/tomllib/_parser.py:629:38: Name `datetime_obj` used when possibly not defined",
];

fn get_test_file(name: &str) -> TestFile {
    let path = format!("tomllib/{name}");
    let url = format!("{TOMLLIB_312_URL}/{name}");
    TestFile::try_download(&path, &url).unwrap()
}

fn tomllib_path(filename: &str) -> SystemPathBuf {
    SystemPathBuf::from(format!("/src/tomllib/{filename}").as_str())
}

fn setup_case() -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();

    let tomllib_filenames = ["__init__.py", "_parser.py", "_re.py", "_types.py"];
    fs.write_files(tomllib_filenames.iter().map(|filename| {
        (
            tomllib_path(filename),
            get_test_file(filename).code().to_string(),
        )
    }))
    .unwrap();

    let src_root = SystemPath::new("/src");
    let metadata = WorkspaceMetadata::from_path(
        src_root,
        &system,
        Some(Configuration {
            target_version: Some(PythonVersion::PY312),
            ..Configuration::default()
        }),
    )
    .unwrap();

    let mut db = RootDatabase::new(metadata, system).unwrap();

    let tomllib_files: FxHashSet<File> = tomllib_filenames
        .iter()
        .map(|filename| system_path_to_file(&db, tomllib_path(filename)).unwrap())
        .collect();
    db.workspace().set_open_files(&mut db, tomllib_files);

    let re_path = tomllib_path("_re.py");
    let re = system_path_to_file(&db, &re_path).unwrap();
    Case {
        db,
        fs,
        re,
        re_path,
    }
}

static RAYON_INITIALIZED: std::sync::Once = std::sync::Once::new();

fn setup_rayon() {
    // Initialize the rayon thread pool outside the benchmark because it has a significant cost.
    // We limit the thread pool to only one (the current thread) because we're focused on
    // where red knot spends time and less about how well the code runs concurrently.
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
    setup_rayon();

    criterion.bench_function("red_knot_check_file[incremental]", |b| {
        b.iter_batched_ref(
            || {
                let case = setup_case();
                case.db.check().unwrap();

                case.fs
                    .write_file(
                        &case.re_path,
                        format!("{}\n# A comment\n", source_text(&case.db, case.re).as_str()),
                    )
                    .unwrap();

                case
            },
            |case| {
                let Case { db, .. } = case;

                db.apply_changes(
                    vec![ChangeEvent::Changed {
                        path: case.re_path.clone(),
                        kind: ChangedKind::FileContent,
                    }],
                    None,
                );

                let result = db.check().unwrap();

                assert_eq!(result, EXPECTED_DIAGNOSTICS);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_cold(criterion: &mut Criterion) {
    setup_rayon();

    criterion.bench_function("red_knot_check_file[cold]", |b| {
        b.iter_batched_ref(
            setup_case,
            |case| {
                let Case { db, .. } = case;
                let result = db.check().unwrap();

                assert_eq!(result, EXPECTED_DIAGNOSTICS);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(check_file, benchmark_cold, benchmark_incremental);
criterion_main!(check_file);
