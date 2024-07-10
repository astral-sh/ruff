#![allow(clippy::disallowed_names)]

use red_knot::program::Program;
use red_knot::Workspace;
use red_knot_module_resolver::{
    set_module_resolution_settings, RawModuleResolutionSettings, TargetVersion,
};
use ruff_benchmark::criterion::{
    criterion_group, criterion_main, BatchSize, Criterion, Throughput,
};
use ruff_db::files::{system_path_to_file, File};
use ruff_db::parsed::parsed_module;
use ruff_db::system::{MemoryFileSystem, SystemPath, TestSystem};
use ruff_db::Upcast;

static FOO_CODE: &str = r#"
import typing

from bar import Bar

class Foo(Bar):
    def foo() -> str:
        return "foo"

    @typing.override
    def bar() -> str:
        return "foo_bar"
"#;

static BAR_CODE: &str = r#"
class Bar:
    def bar() -> str:
        return "bar"

    def random(arg: int) -> int:
        if arg == 1:
            return 48472783
        if arg < 10:
            return 20
        return 36673
"#;

static TYPING_CODE: &str = r#"
def override(): ...
"#;

struct Case {
    program: Program,
    fs: MemoryFileSystem,
    foo: File,
    bar: File,
    typing: File,
}

fn setup_case() -> Case {
    let system = TestSystem::default();
    let fs = system.memory_file_system().clone();
    let foo_path = SystemPath::new("/src/foo.py");
    let bar_path = SystemPath::new("/src/bar.py");
    let typing_path = SystemPath::new("/src/typing.pyi");
    fs.write_files([
        (foo_path, FOO_CODE),
        (bar_path, BAR_CODE),
        (typing_path, TYPING_CODE),
    ])
    .unwrap();

    let workspace_root = SystemPath::new("/src");
    let workspace = Workspace::new(workspace_root.to_path_buf());

    let mut program = Program::new(workspace, system);
    let foo = system_path_to_file(&program, foo_path).unwrap();

    set_module_resolution_settings(
        &mut program,
        RawModuleResolutionSettings {
            extra_paths: vec![],
            workspace_root: workspace_root.to_path_buf(),
            site_packages: None,
            custom_typeshed: None,
            target_version: TargetVersion::Py38,
        },
    );

    program.workspace_mut().open_file(foo);

    let bar = system_path_to_file(&program, bar_path).unwrap();
    let typing = system_path_to_file(&program, typing_path).unwrap();

    Case {
        program,
        fs,
        foo,
        bar,
        typing,
    }
}

fn benchmark_without_parse(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("red_knot/check_file");
    group.throughput(Throughput::Bytes(FOO_CODE.len() as u64));

    group.bench_function("red_knot_check_file[without_parse]", |b| {
        b.iter_batched_ref(
            || {
                let case = setup_case();
                // Pre-parse the module to only measure the semantic time.
                parsed_module(case.program.upcast(), case.foo);
                parsed_module(case.program.upcast(), case.bar);
                parsed_module(case.program.upcast(), case.typing);
                case
            },
            |case| {
                let Case { program, foo, .. } = case;
                let result = program.check_file(*foo).unwrap();

                assert_eq!(result.as_slice(), [] as [String; 0]);
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn benchmark_incremental(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("red_knot/check_file");
    group.throughput(Throughput::Bytes(FOO_CODE.len() as u64));

    group.bench_function("red_knot_check_file[incremental]", |b| {
        b.iter_batched_ref(
            || {
                let mut case = setup_case();
                case.program.check_file(case.foo).unwrap();

                case.fs
                    .write_file(
                        SystemPath::new("/src/foo.py"),
                        format!("{BAR_CODE}\n# A comment\n"),
                    )
                    .unwrap();

                case.bar.touch(&mut case.program);
                case
            },
            |case| {
                let Case { program, foo, .. } = case;
                let result = program.check_file(*foo).unwrap();

                assert_eq!(result.as_slice(), [] as [String; 0]);
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn benchmark_cold(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("red_knot/check_file");
    group.throughput(Throughput::Bytes(FOO_CODE.len() as u64));

    group.bench_function("red_knot_check_file[cold]", |b| {
        b.iter_batched_ref(
            setup_case,
            |case| {
                let Case { program, foo, .. } = case;
                let result = program.check_file(*foo).unwrap();

                assert_eq!(result.as_slice(), [] as [String; 0]);
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(cold, benchmark_without_parse);
criterion_group!(without_parse, benchmark_cold);
criterion_group!(incremental, benchmark_incremental);
criterion_main!(without_parse, cold, incremental);
