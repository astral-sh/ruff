use std::{collections::HashMap, time::Duration};

use lsp_types::{FileRename, MessageType, ShowMessageNotification, Uri, WorkspaceEdit};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::notebook::NotebookBuilder;
use crate::{TestServer, TestServerBuilder};

#[test]
fn batch_includes_moved_and_open_sources() {
    let mut server = TestServerBuilder::new()
        .and_then(|builder| {
            builder.with_files([
                ("ty.toml", "[src]\nexclude = [\"old.py\"]\n"),
                (
                    "old.py",
                    "import other
other",
                ),
                ("other.py", ""),
                (
                    "consumer.py",
                    "import old
old",
                ),
            ])
        })
        .expect("test workspace should be created")
        .build()
        .wait_until_workspaces_are_initialized();
    let mut notebook = NotebookBuilder::virtual_file("consumer.ipynb");
    let cell = notebook.add_python_cell("import other");
    notebook.open(&mut server);
    server.collect_publish_diagnostic_notifications(1);

    let edit = rename_edit(
        &mut server,
        &[("old.py", "new.py"), ("other.py", "renamed.py")],
    )
    .expect("the supported batch to produce edits");
    assert_edits(
        &edit,
        &server.file_uri("consumer.py"),
        &[(0, 7, 0, 10, "new"), (1, 0, 1, 3, "new")],
    );
    assert_edits(
        &edit,
        &server.file_uri("old.py"),
        &[(0, 7, 0, 12, "renamed"), (1, 0, 1, 5, "renamed")],
    );
    assert_edits(&edit, &cell, &[(0, 7, 0, 12, "renamed")]);
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(3));
}

#[test]
fn unsupported_entries_do_not_suppress_independent_edits() {
    let mut server = TestServerBuilder::new()
        .and_then(|builder| builder.with_workspace(SystemPath::new("repo/a"), None))
        .and_then(|builder| builder.with_workspace(SystemPath::new("repo/b"), None))
        .and_then(|builder| {
            builder.with_files([
                ("repo/a/old.py", ""),
                ("repo/a/use.py", "import old"),
                ("repo/a/notes.txt", ""),
                ("repo/b/old.py", ""),
                ("repo/b/use.py", "import old"),
            ])
        })
        .expect("test workspaces should be created")
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[
            ("repo/a/old.py", "repo/a/new.py"),
            ("repo/a/notes.txt", "repo/a/new.txt"),
        ],
    )
    .expect("an unrelated rename should not affect the supported entry");
    assert_edits(
        &edit,
        &server.file_uri("repo/a/use.py"),
        &[(0, 7, 0, 10, "new")],
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
        &[(0, 7, 0, 10, "new")],
    );
    assert_edits(
        &edit,
        &server.file_uri("repo/b/use.py"),
        &[(0, 7, 0, 10, "new")],
    );
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(2));

    let edit = rename_edit(
        &mut server,
        &[
            ("repo/a/old.py", "repo/b/cross.py"),
            ("repo/b/old.py", "repo/b/new.py"),
        ],
    )
    .expect("the independent file rename to survive a cross-workspace move");
    assert_edits(
        &edit,
        &server.file_uri("repo/b/use.py"),
        &[(0, 7, 0, 10, "new")],
    );
    assert_eq!(edit.changes.as_ref().map(HashMap::len), Some(1));
    assert_incomplete_warning(&mut server);
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
            builder.with_files([("repo/old.py", ""), ("repo/use.py", "import old")])
        })
        .expect("test workspace should be created")
        .build()
        .wait_until_workspaces_are_initialized();

    assert!(rename_edit(&mut server, &[("repo/old.py", "repo/new.py")]).is_none());
    assert_incomplete_warning(&mut server);
}

#[track_caller]
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
        .map(|(old, new)| FileRename::new(server.file_uri(old), server.file_uri(new)))
        .collect();
    server.will_rename_files(files)
}

#[track_caller]
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
    assert_eq!(actual, expected, "{uri:?}");
}
