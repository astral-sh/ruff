use std::collections::HashMap;

use lsp_types::{
    DidOpenTextDocumentNotification, DidOpenTextDocumentParams, FileRename, LanguageKind,
    TextDocumentItem, TextEdit, Uri,
};
use ruff_db::system::SystemPath;

use crate::notebook::NotebookBuilder;
use crate::{TestServer, TestServerBuilder};

#[test]
fn batch_file_and_namespace_directory() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file("old_module.py", "x = 1\n")?
        .with_file("old_ns/sub.py", "x = 2\n")?
        .with_file(
            "consumer.py",
            "import old_module\nimport old_ns.sub\nprint(old_module.x, old_ns.sub.x)\n",
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    let mut changes = rename_changes(
        &mut server,
        &[("old_module.py", "new_module.py"), ("old_ns", "new_ns")],
    );
    let edits = changes
        .remove(&server.file_uri("consumer.py"))
        .expect("batch to edit the consumer");

    assert_eq!(
        replacement_texts(&edits),
        ["new_module", "new_ns.sub", "new_module", "new_ns.sub"]
    );
    assert!(changes.is_empty());

    Ok(())
}

#[test]
fn batch_is_grouped_by_owning_workspace() -> anyhow::Result<()> {
    let workspace_a = SystemPath::new("repo/a");
    let workspace_b = SystemPath::new("repo/b");
    let mut server = TestServerBuilder::new()?
        .with_file("repo/pyproject.toml", "[tool.ty]\n")?
        .with_workspace(workspace_a, None)?
        .with_file("repo/a/foo/__init__.py", "")?
        .with_file("repo/a/foo/moved.py", "x = 1\n")?
        .with_file("repo/a/bar/__init__.py", "")?
        .with_file(
            "repo/a/consumer.py",
            "from a.foo import moved\nprint(moved.x)\n",
        )?
        .with_workspace(workspace_b, None)?
        .with_file("repo/b/old_b.py", "x = 2\n")?
        .with_file(
            "repo/b/consumer.py",
            "from a.foo import moved\nimport b.old_b\nprint(moved.x, b.old_b.x)\n",
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    let mut changes = rename_changes(
        &mut server,
        &[
            ("repo/a/foo/moved.py", "repo/a/bar/new.py"),
            ("repo/b/old_b.py", "repo/b/new_b.py"),
        ],
    );
    let a_edits = changes
        .remove(&server.file_uri("repo/a/consumer.py"))
        .expect("the first workspace to receive its edits");
    let b_edits = changes
        .remove(&server.file_uri("repo/b/consumer.py"))
        .expect("the second workspace to receive its edits");

    assert_eq!(replacement_texts(&a_edits), ["a.bar", "new", "new"]);
    assert_eq!(replacement_texts(&b_edits), ["b.new_b", "b.new_b"]);
    assert!(changes.is_empty());

    Ok(())
}

#[test]
fn open_and_excluded_files_are_candidates() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_file(
            "pyproject.toml",
            "[tool.ty.src]\nexclude = [\"old_pkg/excluded.py\"]\n",
        )?
        .with_file("old_pkg/__init__.py", "")?
        .with_file("old_pkg/sub.py", "x = 1\n")?
        .with_file(
            "old_pkg/excluded.py",
            "import old_pkg.sub\nprint(old_pkg.sub.x)\n",
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    let mut notebook = NotebookBuilder::virtual_file("consumer.ipynb");
    let first_cell_uri = notebook.add_python_cell("import old_pkg.sub\nprint(old_pkg.sub.x)\n");
    let second_cell_uri = notebook.add_python_cell("import old_pkg.sub\nprint(old_pkg.sub.x)\n");
    notebook.open(&mut server);
    server.collect_publish_diagnostic_notifications(2);

    let file_uri = server.file_uri("consumer.py");
    let untitled_uri = Uri::parse(&format!("untitled://{}", file_uri.path())).unwrap();
    server.send_notification::<DidOpenTextDocumentNotification>(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: untitled_uri.clone(),
            language_id: LanguageKind::Python,
            version: 1,
            text: "import old_pkg.sub\nprint(old_pkg.sub.x)\n".to_string(),
        },
    });

    let mut changes = rename_changes(&mut server, &[("old_pkg", "new_pkg")]);
    for uri in [
        server.file_uri("old_pkg/excluded.py"),
        first_cell_uri,
        second_cell_uri,
        untitled_uri,
    ] {
        let edits = changes.remove(&uri).expect("candidate to receive edits");
        assert_eq!(replacement_texts(&edits), ["new_pkg.sub", "new_pkg.sub"]);
    }
    assert!(changes.is_empty());

    Ok(())
}

fn rename_changes(
    server: &mut TestServer,
    renames: &[(&str, &str)],
) -> HashMap<Uri, Vec<TextEdit>> {
    let renames = renames
        .iter()
        .map(|(old, new)| FileRename {
            old_uri: server.file_uri(old).to_string(),
            new_uri: server.file_uri(new).to_string(),
        })
        .collect();
    server
        .will_rename_files(renames)
        .and_then(|edit| edit.changes)
        .expect("rename to produce workspace changes")
}

fn replacement_texts(edits: &[TextEdit]) -> Vec<&str> {
    edits.iter().map(|edit| edit.new_text.as_str()).collect()
}
