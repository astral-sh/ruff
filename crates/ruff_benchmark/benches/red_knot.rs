#![allow(clippy::disallowed_names)]

use red_knot_python_semantic::PythonVersion;
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::watch::{ChangedKind, ChangeEvent};
use red_knot_workspace::workspace::settings::Configuration;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_benchmark::criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ruff_benchmark::TestFile;
use ruff_db::files::{File, system_path_to_file};
use ruff_db::source::source_text;
use ruff_db::system::{MemoryFileSystem, SystemPath, TestSystem};

struct Case {
    db: RootDatabase,
    fs: MemoryFileSystem,
    re: File,
    re_path: &'static SystemPath,
}

const TOMLLIB_312_URL: &str = "https://raw.githubusercontent.com/python/cpython/8e8a4baf652f6e1cee7acde9d78c4b6154539748/Lib/tomllib";

// This first "unresolved import" is because we don't understand `*` imports yet.
// The following "unresolved import" violations are because we can't distinguish currently from
// "Symbol exists in the module but its type is unknown" and
// "Symbol does not exist in the module"
static EXPECTED_DIAGNOSTICS: &[&str] = &[
    "/src/tomllib/_parser.py:7:29: Could not resolve import of 'Iterable' from 'collections.abc'",
    "/src/tomllib/_parser.py:10:20: Could not resolve import of 'Any' from 'typing'",
    "/src/tomllib/_parser.py:13:5: Could not resolve import of 'RE_DATETIME' from '._re'",
    "/src/tomllib/_parser.py:14:5: Could not resolve import of 'RE_LOCALTIME' from '._re'",
    "/src/tomllib/_parser.py:15:5: Could not resolve import of 'RE_NUMBER' from '._re'",
    "/src/tomllib/_parser.py:20:21: Could not resolve import of 'Key' from '._types'",
    "/src/tomllib/_parser.py:20:26: Could not resolve import of 'ParseFloat' from '._types'",
    "Line 69 is too long (89 characters)",
    "Use double quotes for strings",
    "Use double quotes for strings",
    "Use double quotes for strings",
    "Use double quotes for strings",
    "Use double quotes for strings",
    "Use double quotes for strings",
    "Use double quotes for strings",
    "/src/tomllib/_parser.py:153:22: Name 'key' used when not defined.",
    "/src/tomllib/_parser.py:153:27: Name 'flag' used when not defined.",
    "/src/tomllib/_parser.py:159:16: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:161:25: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:168:16: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:169:22: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:170:25: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:180:16: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:182:31: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:206:16: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:207:22: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:208:25: Name 'k' used when not defined.",
    "/src/tomllib/_parser.py:330:32: Name 'header' used when not defined.",
    "/src/tomllib/_parser.py:330:41: Name 'key' used when not defined.",
    "/src/tomllib/_parser.py:333:26: Name 'cont_key' used when not defined.",
    "/src/tomllib/_parser.py:334:71: Name 'cont_key' used when not defined.",
    "/src/tomllib/_parser.py:337:31: Name 'cont_key' used when not defined.",
    "/src/tomllib/_parser.py:628:75: Name 'e' used when not defined.",
    "/src/tomllib/_parser.py:686:23: Name 'parse_float' used when not defined.",
];

fn get_test_file(name: &str) -> TestFile {
    let path = format!("tomllib/{name}");
    let url = format!("{TOMLLIB_312_URL}/{name}");
    TestFile::try_download(&path, &url).unwrap()
}

fn setup_case() -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();
    let parser_path = SystemPath::new("/src/tomllib/_parser.py");
    let re_path = SystemPath::new("/src/tomllib/_re.py");
    fs.write_files([
        (
            SystemPath::new("/src/tomllib/__init__.py"),
            get_test_file("__init__.py").code(),
        ),
        (parser_path, get_test_file("_parser.py").code()),
        (re_path, get_test_file("_re.py").code()),
        (
            SystemPath::new("/src/tomllib/_types.py"),
            get_test_file("_types.py").code(),
        ),
    ])
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
    let parser = system_path_to_file(&db, parser_path).unwrap();

    db.workspace().open_file(&mut db, parser);

    let re = system_path_to_file(&db, re_path).unwrap();

    Case {
        db,
        fs,
        re,
        re_path,
    }
}

fn benchmark_incremental(criterion: &mut Criterion) {
    criterion.bench_function("red_knot_check_file[incremental]", |b| {
        b.iter_batched_ref(
            || {
                let case = setup_case();
                case.db.check().unwrap();

                case.fs
                    .write_file(
                        case.re_path,
                        format!("{}\n# A comment\n", source_text(&case.db, case.re).as_str()),
                    )
                    .unwrap();

                case
            },
            |case| {
                let Case { db, .. } = case;

                db.apply_changes(vec![ChangeEvent::Changed {
                    path: case.re_path.to_path_buf(),
                    kind: ChangedKind::FileContent,
                }]);

                let result = db.check().unwrap();

                assert_eq!(result, EXPECTED_DIAGNOSTICS);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_cold(criterion: &mut Criterion) {
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
