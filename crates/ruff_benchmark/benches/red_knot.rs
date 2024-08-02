#![allow(clippy::disallowed_names)]

use codspeed_criterion_compat::{criterion_group, criterion_main, BatchSize, Criterion};

use red_knot_workspace::db::RootDatabase;
use red_knot_workspace::workspace::WorkspaceMetadata;
use ruff_benchmark::TestFile;
use ruff_db::files::{system_path_to_file, vendored_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::program::{ProgramSettings, SearchPathSettings, TargetVersion};
use ruff_db::source::source_text;
use ruff_db::system::{MemoryFileSystem, SystemPath, TestSystem};
use ruff_db::vendored::VendoredPath;
use ruff_db::Upcast;

struct Case {
    db: RootDatabase,
    fs: MemoryFileSystem,
    parser: File,
    re: File,
    types: File,
    builtins: File,
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
    let parser_path = SystemPath::new("/src/parser.py");
    let re_path = SystemPath::new("/src/_re.py");
    let types_path = SystemPath::new("/src/_types.py");
    let builtins_path = VendoredPath::new("stdlib/builtins.pyi");
    fs.write_files([
        (parser_path, get_test_file("_parser.py").code()),
        (re_path, get_test_file("_re.py").code()),
        (types_path, get_test_file("_types.py").code()),
    ])
    .unwrap();

    let workspace_root = SystemPath::new("/src");
    let metadata = WorkspaceMetadata::from_path(workspace_root, &system).unwrap();
    let settings = ProgramSettings {
        target_version: TargetVersion::Py312,
        search_paths: SearchPathSettings {
            extra_paths: vec![],
            workspace_root: workspace_root.to_path_buf(),
            site_packages: vec![],
            custom_typeshed: None,
        },
    };

    let mut db = RootDatabase::new(metadata, settings, system);
    let parser = system_path_to_file(&db, parser_path).unwrap();

    db.workspace().open_file(&mut db, parser);

    let re = system_path_to_file(&db, re_path).unwrap();
    let types = system_path_to_file(&db, types_path).unwrap();
    let builtins = vendored_path_to_file(&db, builtins_path).unwrap();

    Case {
        db,
        fs,
        parser,
        re,
        types,
        builtins,
    }
}

fn benchmark_without_parse(criterion: &mut Criterion) {
    criterion.bench_function("red_knot_check_file[without_parse]", |b| {
        b.iter_batched_ref(
            || {
                let case = setup_case();
                // Pre-parse the modules to only measure the semantic time.
                parsed_module(case.db.upcast(), case.parser);
                parsed_module(case.db.upcast(), case.re);
                parsed_module(case.db.upcast(), case.types);
                parsed_module(case.db.upcast(), case.builtins);
                case
            },
            |case| {
                let Case { db, parser, .. } = case;
                let _ = db.check_file(*parser).unwrap();

                // assert_eq!(result.as_slice(), [] as [String; 0]);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_incremental(criterion: &mut Criterion) {
    criterion.bench_function("red_knot_check_file[incremental]", |b| {
        b.iter_batched_ref(
            || {
                let mut case = setup_case();
                case.db.check_file(case.parser).unwrap();

                case.fs
                    .write_file(
                        SystemPath::new("/src/_re.py"),
                        format!("{}\n# A comment\n", source_text(&case.db, case.re).as_str()),
                    )
                    .unwrap();

                case.re.sync(&mut case.db);
                case
            },
            |case| {
                let Case { db, parser, .. } = case;
                let _ = db.check_file(*parser).unwrap();

                // assert_eq!(result.as_slice(), [] as [String; 0]);
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
                let _ = db.check_file(*parser).unwrap();

                // assert_eq!(result.as_slice(), [] as [String; 0]);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    check_file,
    benchmark_cold,
    benchmark_without_parse,
    benchmark_incremental
);
criterion_main!(check_file);
