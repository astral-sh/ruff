use red_knot::db::RootDatabase;
use red_knot::workspace::WorkspaceMetadata;
use ruff_db::files::system_path_to_file;
use ruff_db::program::{ProgramSettings, SearchPathSettings, TargetVersion};
use ruff_db::system::{SystemPath, TestSystem};

static FOO_CODE: &str = r#"
import typing

from bar import Bar

class Foo(Bar):
    def foo() -> object:
        return "foo"

    @typing.override
    def bar() -> object:
        return "foo_bar"
"#;

static BAR_CODE: &str = r#"
class Bar:
    def bar() -> object:
        return "bar"

    def random(arg: int) -> int:
        if arg == 1:
            return 48472783
        if arg < 10:
            return 20
        while arg < 50:
            arg += 1
        return 36673
"#;

static TYPING_CODE: &str = r#"
def override(): ...
"#;

#[test]
fn incremental() {
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
    let metadata = WorkspaceMetadata::from_path(workspace_root, &system).unwrap();
    let settings = ProgramSettings {
        target_version: TargetVersion::default(),
        search_paths: SearchPathSettings {
            extra_paths: vec![],
            workspace_root: workspace_root.to_path_buf(),
            site_packages: None,
            custom_typeshed: None,
        },
    };

    let mut db = RootDatabase::new(metadata, settings, system);
    let foo = system_path_to_file(&db, foo_path).unwrap();

    db.workspace().open_file(&mut db, foo);
    let bar = system_path_to_file(&db, bar_path).unwrap();
    db.check_file(foo).unwrap();

    fs.write_file(
        SystemPath::new("/src/bar.py"),
        format!("{BAR_CODE}\n# A comment\n"),
    )
    .unwrap();

    bar.sync(&mut db);

    let result = db.check_file(foo).unwrap();
    assert_eq!(result.as_slice(), [] as [String; 0]);
}
