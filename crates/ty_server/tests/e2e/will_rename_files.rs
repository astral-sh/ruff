use std::{collections::HashMap, time::Duration};

use lsp_types::{FileRename, MessageType, ShowMessageNotification, Uri, WorkspaceEdit};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::notebook::NotebookBuilder;
use crate::{TestServer, TestServerBuilder};

#[test]
fn batch_includes_moved_and_open_sources() {
    let sub = r#"import old_pkg
from . import helper
old_pkg.x
"#;
    let consumer = r#"import old
from old_pkg import sub
old.x
"#;
    let mut server = TestServerBuilder::new()
        .and_then(|builder| {
            builder.with_file("ty.toml", "[src]\nexclude = [\"old.py\", \"old_pkg/\"]\n")
        })
        .and_then(|builder| {
            builder.with_file(
                "old.py",
                r#"import old_pkg
old_pkg.x
"#,
            )
        })
        .and_then(|builder| builder.with_file("old_pkg/__init__.py", ""))
        .and_then(|builder| {
            builder.with_file(
                "old_pkg/helper.py",
                r#"x = 2
"#,
            )
        })
        .and_then(|builder| builder.with_file("old_pkg/sub.py", sub))
        .and_then(|builder| builder.with_file("consumer.py", consumer))
        .expect("test workspace should be created")
        .build()
        .wait_until_workspaces_are_initialized();
    let mut notebook = NotebookBuilder::virtual_file("consumer.ipynb");
    let cell = notebook.add_python_cell(
        r#"import old_pkg.sub as old_pkg

def f():
    print(old_pkg.x)
"#,
    );
    notebook.open(&mut server);
    server.collect_publish_diagnostic_notifications(1);

    let edit = rename_edit(&mut server, &[("old.py", "new.py"), ("old_pkg", "new_pkg")])
        .expect("the supported batch to produce edits");
    assert_edits(
        &edit,
        &server.file_uri("consumer.py"),
        &[
            (0, 7, 0, 10, "new"),
            (1, 5, 1, 12, "new_pkg"),
            (2, 0, 2, 3, "new"),
        ],
    );
    assert_edits(
        &edit,
        &server.file_uri("old_pkg/sub.py"),
        &[(0, 7, 0, 14, "new_pkg"), (2, 0, 2, 7, "new_pkg")],
    );
    assert_edits(
        &edit,
        &server.file_uri("old.py"),
        &[(0, 7, 0, 14, "new_pkg"), (1, 0, 1, 7, "new_pkg")],
    );
    assert_edits(&edit, &cell, &[(0, 7, 0, 18, "new_pkg.sub")]);
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(4));
}

#[test]
fn unsupported_entries_do_not_suppress_independent_edits() {
    let a = SystemPath::new("repo/a");
    let b = SystemPath::new("repo/b");
    let mut server = TestServerBuilder::new()
        .and_then(|builder| builder.with_workspace(a, None))
        .and_then(|builder| builder.with_workspace(b, None))
        .and_then(|builder| {
            builder.with_file(
                "repo/a/old.py",
                r#"x = 1
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/a/oldns/mod.py",
                r#"x = 2
"#,
            )
        })
        .and_then(|builder| builder.with_file("repo/a/notes.txt", "notes\n"))
        .and_then(|builder| builder.with_file("repo/a/empty/.gitkeep", ""))
        .and_then(|builder| {
            builder.with_file(
                "repo/a/use.py",
                r#"import old
import oldns.mod
old.x
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/b/old.py",
                r#"x = 3
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/b/use.py",
                r#"import old
old.x
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/outside.py",
                r#"x = 4
"#,
            )
        })
        .expect("test workspaces should be created")
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[
            ("repo/a/old.py", "repo/a/new.py"),
            ("repo/a/notes.txt", "repo/a/new.txt"),
            ("repo/a/empty", "repo/a/renamed_empty"),
            (
                "file://remote.example/%FFnotes%2Etxt",
                "file://remote.example/%FFnew%2Etxt",
            ),
        ],
    )
    .expect("an unrelated rename should not affect the supported entry");
    assert_edits(
        &edit,
        &server.file_uri("repo/a/use.py"),
        &[(0, 7, 0, 10, "new"), (2, 0, 2, 3, "new")],
    );
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(1));
    assert!(
        server
            .try_await_notification::<ShowMessageNotification>(Some(Duration::from_millis(10)))
            .is_err()
    );
    let edit = rename_edit(
        &mut server,
        &[
            ("repo/a/old.py", "repo/a/new.py"),
            ("repo/a/oldns", "repo/a/newns"),
        ],
    )
    .expect("the supported file rename to survive an unsupported namespace package");
    assert_edits(
        &edit,
        &server.file_uri("repo/a/use.py"),
        &[(0, 7, 0, 10, "new"), (2, 0, 2, 3, "new")],
    );
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(1));
    assert!(
        server
            .try_await_notification::<ShowMessageNotification>(Some(Duration::from_millis(10)))
            .is_err()
    );

    let edit = rename_edit(
        &mut server,
        &[
            ("repo/a/old.py", "repo/a/new.py"),
            ("repo/b/old.py", "repo/b/new.py"),
        ],
    )
    .expect("independent workspaces to contribute edits");
    assert_edits(
        &edit,
        &server.file_uri("repo/a/use.py"),
        &[(0, 7, 0, 10, "new"), (2, 0, 2, 3, "new")],
    );
    assert_edits(
        &edit,
        &server.file_uri("repo/b/use.py"),
        &[(0, 7, 0, 10, "new"), (1, 0, 1, 3, "new")],
    );
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(2));

    for unsupported in [
        (
            "file://remote.example/old.py",
            "file://remote.example/new.py",
        ),
        ("repo/a/old.py", "file://remote.example/new.py"),
        ("repo/outside.py", "repo/outside_new.py"),
        ("repo/a/old.py", "repo/b/cross.py"),
        ("repo/a/old.py", "repo/a/new.pyi"),
    ] {
        let edit = rename_edit(
            &mut server,
            &[unsupported, ("repo/b/old.py", "repo/b/new.py")],
        )
        .expect("the independent file rename to survive");
        assert_edits(
            &edit,
            &server.file_uri("repo/b/use.py"),
            &[(0, 7, 0, 10, "new"), (1, 0, 1, 3, "new")],
        );
        assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(1));
        assert_incomplete_warning(&mut server);
    }
}

#[test]
fn coordinated_facets_leave_exports_unchanged() {
    let mut server = TestServerBuilder::new()
        .and_then(|builder| builder.with_file("pkg/__init__.py", ""))
        .and_then(|builder| {
            builder.with_file(
                "pkg/old.py",
                r#"class C: ...
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "pkg/old.pyi",
                r#"class C: ...
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "use.py",
                r#"from pkg import old
__all__ = ['old']
print(old.C)
"#,
            )
        })
        .expect("test workspace should be created")
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[("pkg/old.py", "pkg/new.py"), ("pkg/old.pyi", "pkg/new.pyi")],
    )
    .expect("the coordinated module rename to produce edits");
    assert_edits(
        &edit,
        &server.file_uri("use.py"),
        &[(0, 16, 0, 19, "new"), (2, 6, 2, 9, "new")],
    );
    assert!(
        server
            .try_await_notification::<ShowMessageNotification>(Some(Duration::from_millis(10)))
            .is_err()
    );
}

#[test]
fn disabled_workspace_reports_an_incomplete_rename() {
    let mut server = TestServerBuilder::new()
        .and_then(|builder| {
            builder.with_workspace(
                SystemPath::new("repo"),
                Some(ClientOptions::default().with_disable_language_services(true)),
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/old.py",
                r#"x = 1
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/old.pyi",
                r#"x: int
"#,
            )
        })
        .and_then(|builder| {
            builder.with_file(
                "repo/use.py",
                r#"import old
old.x
"#,
            )
        })
        .expect("test workspace should be created")
        .build()
        .wait_until_workspaces_are_initialized();

    assert!(rename_edit(&mut server, &[("repo/old.py", "repo/new.py")]).is_none());
    assert_incomplete_warning(&mut server);

    assert!(rename_edit(&mut server, &[("repo/old.pyi", "repo/new.pyi")]).is_none());
    assert_incomplete_warning(&mut server);
}

fn assert_incomplete_warning(server: &mut TestServer) {
    let warning = server.await_notification::<ShowMessageNotification>();
    assert_eq!(warning.kind, MessageType::Warning);
    assert_eq!(
        warning.message,
        "ty could not safely update all affected Python code. Some imports, references, or exports may remain unchanged after this file operation."
    );
}

fn rename_edit(server: &mut TestServer, renames: &[(&str, &str)]) -> Option<WorkspaceEdit> {
    let files = renames
        .iter()
        .map(|(old, new)| FileRename::new(rename_uri(server, old), rename_uri(server, new)))
        .collect();
    server.will_rename_files(files)
}

fn rename_uri(server: &TestServer, path: &str) -> Uri {
    if path.contains("://") {
        Uri::parse(path).expect("test rename URI to be valid")
    } else {
        server.file_uri(path)
    }
}

fn assert_edits(edit: &WorkspaceEdit, uri: &Uri, expected: &[(u32, u32, u32, u32, &str)]) {
    let edits = edit
        .changes
        .as_ref()
        .and_then(|changes| changes.get(uri))
        .expect("workspace edit to contain edits for the URI");
    let actual: Vec<_> = edits
        .iter()
        .map(|edit| {
            (
                edit.range.start.line,
                edit.range.start.character,
                edit.range.end.line,
                edit.range.end.character,
                edit.new_text.as_str(),
            )
        })
        .collect();
    assert_eq!(actual, expected);
}
