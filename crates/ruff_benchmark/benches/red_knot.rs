#![allow(clippy::disallowed_names)]

use red_knot_python_semantic::{ProgramSettings, PythonVersion, SearchPathSettings};
use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_benchmark::criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use ruff_benchmark::TestFile;
use ruff_db::files::{system_path_to_file, File};
use ruff_db::source::source_text;
use ruff_db::system::{MemoryFileSystem, SystemPath, TestSystem};

struct Case {
    db: RootDatabase,
    fs: MemoryFileSystem,
    parser: File,
    re: File,
    re_path: &'static SystemPath,
}

const TOMLLIB_312_URL: &str = "https://raw.githubusercontent.com/python/cpython/8e8a4baf652f6e1cee7acde9d78c4b6154539748/Lib/tomllib";

fn get_test_file(name: &str) -> TestFile {
    let path = format!("tomllib/{name}");
    let url = format!("{TOMLLIB_312_URL}/{name}");
    TestFile::try_download(&path, &url).unwrap()
}

fn setup_case() -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();
    let init_path = SystemPath::new("/src/tomllib/__init__.py");
    let parser_path = SystemPath::new("/src/tomllib/_parser.py");
    let re_path = SystemPath::new("/src/tomllib/_re.py");
    let types_path = SystemPath::new("/src/tomllib/_types.py");
    fs.write_files([
        (init_path, get_test_file("__init__.py").code()),
        (parser_path, get_test_file("_parser.py").code()),
        (re_path, get_test_file("_re.py").code()),
        (types_path, get_test_file("_types.py").code()),
    ])
    .unwrap();

    let src_root = SystemPath::new("/src");
    let metadata = WorkspaceMetadata::from_path(src_root, &system).unwrap();
    let settings = ProgramSettings {
        target_version: PythonVersion::PY312,
        search_paths: SearchPathSettings {
            extra_paths: vec![],
            src_root: src_root.to_path_buf(),
            site_packages: vec![],
            custom_typeshed: None,
        },
    };

    let mut db = RootDatabase::new(metadata, settings, system).unwrap();
    let parser = system_path_to_file(&db, parser_path).unwrap();

    db.workspace().open_file(&mut db, parser);

    let re = system_path_to_file(&db, re_path).unwrap();

    Case {
        db,
        fs,
        parser,
        re,
        re_path,
    }
}

fn benchmark_incremental(criterion: &mut Criterion) {
    criterion.bench_function("red_knot_check_file[incremental]", |b| {
        b.iter_batched_ref(
            || {
                let mut case = setup_case();
                case.db.check_file(case.parser).unwrap();

                case.fs
                    .write_file(
                        case.re_path,
                        format!("{}\n# A comment\n", source_text(&case.db, case.re).as_str()),
                    )
                    .unwrap();

                case.re.sync(&mut case.db);
                case
            },
            |case| {
                let Case { db, parser, .. } = case;
                let result = db.check_file(*parser).unwrap();

                assert_eq!(result.len(), 34);
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
                let Case { db, parser, .. } = case;
                let result = db.check_file(*parser).unwrap();

                assert_eq!(result.len(), 34);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(check_file, benchmark_cold, benchmark_incremental);
criterion_main!(check_file);
