use lsp_types::{FileRename, Position, Range, TextEdit, Uri, WorkspaceEdit};
use ruff_db::system::SystemPath;

use crate::notebook::NotebookBuilder;
use crate::{TestServer, TestServerBuilder};

#[test]
fn batch_updates_only_indexed_sources() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_files([
            ("old.py", ""),
            ("other.py", ""),
            // This consumer stays closed, so it must be found through the project index.
            (
                "consumer.py",
                "import old, other
old",
            ),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    // An open virtual notebook is absent from the project index.
    let mut notebook = NotebookBuilder::virtual_file("consumer.ipynb");
    notebook.add_python_cell("import other");
    notebook.open(&mut server);
    // Consume the cell's diagnostics so the test server has no unclaimed notifications at teardown.
    server.collect_publish_diagnostic_notifications(1);

    // One batch updates the indexed file but not the open notebook cell.
    let edit = rename_edit(
        &mut server,
        &[("old.py", "new.py"), ("other.py", "renamed.py")],
    );

    assert_edits(
        edit.as_ref(),
        &[(
            server.file_uri("consumer.py"),
            &[
                ExpectedEdit {
                    line: 0,
                    columns: 7..10,
                    new_text: "new",
                },
                ExpectedEdit {
                    line: 0,
                    columns: 12..17,
                    new_text: "renamed",
                },
                ExpectedEdit {
                    line: 1,
                    columns: 0..3,
                    new_text: "new",
                },
            ],
        )],
    );

    Ok(())
}

#[test]
fn unsupported_renames_do_not_suppress_supported_edits() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_files([("old.py", ""), ("use.py", "import old"), ("notes.txt", "")])?
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[
            // Non-Python renames are unsupported and must be skipped.
            ("notes.txt", "new.txt"),
            // The Python rename must still update its consumer in the same batch.
            ("old.py", "new.py"),
        ],
    );

    assert_edits(
        edit.as_ref(),
        &[(
            server.file_uri("use.py"),
            &[ExpectedEdit {
                line: 0,
                columns: 7..10,
                new_text: "new",
            }],
        )],
    );

    Ok(())
}

#[test]
fn rename_updates_references_across_workspaces() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("repo/a"), None)?
        .with_workspace(SystemPath::new("repo/b"), None)?
        .with_files([
            ("repo/a/ty.toml", "environment.extra-paths = [\"../b\"]"),
            ("repo/b/ty.toml", "environment.extra-paths = [\"../a\"]"),
            ("repo/a/old.py", ""),
            ("repo/a/use.py", "import old"),
            ("repo/b/use.py", "import old"),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(&mut server, &[("repo/a/old.py", "repo/a/new.py")]);

    assert_edits(
        edit.as_ref(),
        &[
            (
                server.file_uri("repo/a/use.py"),
                &[ExpectedEdit {
                    line: 0,
                    columns: 7..10,
                    new_text: "new",
                }],
            ),
            (
                server.file_uri("repo/b/use.py"),
                &[ExpectedEdit {
                    line: 0,
                    columns: 7..10,
                    new_text: "new",
                }],
            ),
        ],
    );

    Ok(())
}

fn rename_edit(server: &mut TestServer, renames: &[(&str, &str)]) -> Option<WorkspaceEdit> {
    let files = renames
        .iter()
        .map(|(old, new)| FileRename::new(server.file_uri(old), server.file_uri(new)))
        .collect();
    server.will_rename_files(files)
}

/// An expected edit within one line, using zero-based LSP coordinates.
/// The column range excludes its end.
struct ExpectedEdit<'a> {
    line: u32,
    columns: std::ops::Range<u32>,
    new_text: &'a str,
}

/// Asserts the complete response, including the absence of edits for unexpected documents.
#[track_caller]
fn assert_edits(actual: Option<&WorkspaceEdit>, expected: &[(Uri, &[ExpectedEdit<'_>])]) {
    let changes = expected
        .iter()
        .map(|(uri, edits)| {
            let edits = edits
                .iter()
                .map(|edit| {
                    TextEdit::new(
                        Range::new(
                            Position::new(edit.line, edit.columns.start),
                            Position::new(edit.line, edit.columns.end),
                        ),
                        edit.new_text.to_string(),
                    )
                })
                .collect();
            (uri.clone(), edits)
        })
        .collect();
    assert_eq!(actual, Some(&WorkspaceEdit::new(Some(changes), None, None)));
}
