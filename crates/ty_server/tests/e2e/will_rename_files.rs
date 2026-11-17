use lsp_types::{FileRename, Position, Range, TextEdit, Uri, WorkspaceEdit};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

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
fn batch_renames_a_package_and_a_module_file() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_files([
            ("old_pkg/__init__.py", "from old_pkg.helpers import VALUE"),
            ("old_pkg/helpers.py", "VALUE = 1"),
            ("old_file.py", "VALUE = 1"),
            (
                "consumer.py",
                "import old_pkg.helpers, old_file\nold_pkg.helpers.VALUE + old_file.VALUE",
            ),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[("old_pkg", "new_pkg"), ("old_file.py", "new_file.py")],
    );

    assert_edits(
        edit.as_ref(),
        &[
            (
                server.file_uri("old_pkg/__init__.py"),
                &[ExpectedEdit {
                    line: 0,
                    columns: 5..20,
                    new_text: "new_pkg.helpers",
                }],
            ),
            (
                server.file_uri("consumer.py"),
                &[
                    ExpectedEdit {
                        line: 0,
                        columns: 7..22,
                        new_text: "new_pkg.helpers",
                    },
                    ExpectedEdit {
                        line: 0,
                        columns: 24..32,
                        new_text: "new_file",
                    },
                    ExpectedEdit {
                        line: 1,
                        columns: 0..7,
                        new_text: "new_pkg",
                    },
                    ExpectedEdit {
                        line: 1,
                        columns: 24..32,
                        new_text: "new_file",
                    },
                ],
            ),
        ],
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
fn combines_project_edits_and_skips_conflicting_files() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("repo"), None)?
        .with_workspace(SystemPath::new("repo/inner"), None)?
        .with_files([
            ("repo/ty.toml", ""),
            ("repo/inner/ty.toml", "environment.root = [\".\", \"..\"]"),
            ("repo/old.py", ""),
            ("repo/conflict.py", ""),
            ("repo/inner/conflict.py", ""),
            ("repo/use.py", "import old"),
            ("repo/inner/use.py", "import old"),
            ("repo/inner/conflicted.py", "import old, conflict"),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    // The projects agree on `old` but resolve `conflict` in their respective roots.
    // `conflicted.py` must receive no edits, including the agreed replacement for `old`.
    let edit = rename_edit(
        &mut server,
        &[
            ("repo/old.py", "repo/new.py"),
            ("repo/conflict.py", "repo/new_outer.py"),
            ("repo/inner/conflict.py", "repo/inner/new_inner.py"),
        ],
    );

    assert_edits(
        edit.as_ref(),
        &[
            (
                server.file_uri("repo/use.py"),
                &[ExpectedEdit {
                    line: 0,
                    columns: 7..10,
                    new_text: "new",
                }],
            ),
            (
                server.file_uri("repo/inner/use.py"),
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

#[test]
fn disabled_language_services_suppress_file_rename_edits() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_initialization_options(&ClientOptions::default().with_disable_language_services(true))
        .with_files([("old.py", ""), ("use.py", "import old")])?
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(&mut server, &[("old.py", "new.py")]);

    assert_eq!(edit, None);

    Ok(())
}

#[test]
fn file_renames_use_the_source_workspace_setting() -> anyhow::Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("repo"), None)?
        .with_workspace(
            SystemPath::new("repo/disabled"),
            Some(ClientOptions::default().with_disable_language_services(true)),
        )?
        .with_files([
            // Both workspaces discover the project at `repo`, which indexes files in both.
            ("repo/ty.toml", ""),
            ("repo/disabled/__init__.py", ""),
            ("repo/disabled/old.py", ""),
            ("repo/use.py", "from disabled import old"),
            ("repo/shared.py", ""),
            ("repo/disabled/use.py", "import shared"),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    let edit = rename_edit(
        &mut server,
        &[
            ("repo/disabled/old.py", "repo/disabled/new.py"),
            ("repo/shared.py", "repo/renamed.py"),
        ],
    );

    // The disabled source is skipped. The enabled source can still update a disabled importer.
    assert_edits(
        edit.as_ref(),
        &[(
            server.file_uri("repo/disabled/use.py"),
            &[ExpectedEdit {
                line: 0,
                columns: 7..13,
                new_text: "renamed",
            }],
        )],
    );

    Ok(())
}

#[test]
fn external_file_renames_use_the_initialization_setting() -> anyhow::Result<()> {
    for disable_language_services in [None, Some(true)] {
        let mut initialization_options = ClientOptions::default();
        initialization_options.workspace.disable_language_services = disable_language_services;

        let mut server = TestServerBuilder::new()?
            .with_initialization_options(&initialization_options)
            .with_workspace(
                SystemPath::new("app"),
                Some(ClientOptions::default().with_disable_language_services(true)),
            )?
            .with_files([
                ("app/ty.toml", r#"environment.extra-paths = ["../shared"]"#),
                ("shared/old.py", ""),
                ("app/use.py", "import old"),
            ])?
            .build()
            .wait_until_workspaces_are_initialized();

        let edit = rename_edit(&mut server, &[("shared/old.py", "shared/new.py")]);

        if disable_language_services == Some(true) {
            assert_eq!(edit, None);
        } else {
            assert_edits(
                edit.as_ref(),
                &[(
                    server.file_uri("app/use.py"),
                    &[ExpectedEdit {
                        line: 0,
                        columns: 7..10,
                        new_text: "new",
                    }],
                )],
            );
        }
    }

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
