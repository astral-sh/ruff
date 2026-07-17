use std::{collections::HashMap, time::Duration};

use lsp_types::{FileRename, MessageType, ShowMessageNotification, Uri, WorkspaceEdit};
use ruff_db::system::SystemPath;

use crate::notebook::NotebookBuilder;
use crate::{TestServer, TestServerBuilder};

#[test]
fn batch_includes_moved_and_open_sources() -> anyhow::Result<()> {
    let sub = "import old_pkg\nfrom . import helper\nold_pkg.x\n";
    let consumer = "import old\nfrom old_pkg import sub\nold.x\n";
    let mut server = TestServerBuilder::new()?
        .with_file("ty.toml", "[src]\nexclude = [\"old_pkg/\"]\n")?
        .with_file("old.py", "x = 1\n")?
        .with_file("old_pkg/__init__.py", "")?
        .with_file("old_pkg/helper.py", "x = 2\n")?
        .with_file("old_pkg/sub.py", sub)?
        .with_file("consumer.py", consumer)?
        .build()
        .wait_until_workspaces_are_initialized();
    let mut notebook = NotebookBuilder::virtual_file("consumer.ipynb");
    let cell = notebook
        .add_python_cell("import old_pkg.sub as old_pkg\n\ndef f():\n    print(old_pkg.x)\n");
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
    assert_edits(&edit, &cell, &[(0, 7, 0, 18, "new_pkg.sub")]);
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(3));
    Ok(())
}

#[test]
fn unsupported_entries_do_not_suppress_independent_edits() -> anyhow::Result<()> {
    let a = SystemPath::new("repo/a");
    let b = SystemPath::new("repo/b");
    let mut server = TestServerBuilder::new()?
        .with_workspace(a, None)?
        .with_workspace(b, None)?
        .with_file("repo/a/old.py", "x = 1\n")?
        .with_file("repo/a/oldns/mod.py", "x = 2\n")?
        .with_file("repo/a/notes.txt", "notes\n")?
        .with_file("repo/a/use.py", "import old\nimport oldns.mod\nold.x\n")?
        .with_file("repo/b/old.py", "x = 3\n")?
        .with_file("repo/b/use.py", "import old\nold.x\n")?
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[
            ("repo/a/old.py", "repo/a/new.py"),
            ("repo/a/notes.txt", "repo/a/new.txt"),
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
    assert_incomplete_warning(&mut server);

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
    Ok(())
}

fn assert_incomplete_warning(server: &mut TestServer) {
    let warning = server.await_notification::<ShowMessageNotification>();
    assert_eq!(warning.kind, MessageType::Warning);
    assert_eq!(
        warning.message,
        "ty could not safely update every affected Python import or module reference. Some references may remain unchanged after this file operation."
    );
}

fn rename_edit(server: &mut TestServer, renames: &[(&str, &str)]) -> Option<WorkspaceEdit> {
    let files = renames
        .iter()
        .map(|(old, new)| FileRename::new(rename_uri(server, old), rename_uri(server, new)))
        .collect();
    server.will_rename_files(files)
}

fn rename_uri(server: &TestServer, path: &str) -> String {
    if path.contains("://") {
        path.to_string()
    } else {
        server.file_uri(path).to_string()
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
