use anyhow::Result;
use lsp_types::{Position, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

/// Tests that auto-import is enabled by default.
#[test]
fn default_auto_import() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
walktr
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let hints = server.completion_request(&server.file_uri(foo), Position::new(0, 6));

    insta::assert_json_snapshot!(hints, @r#"
    [
      {
        "label": "walktree (import inspect)",
        "kind": 3,
        "sortText": "0",
        "insertText": "walktree",
        "additionalTextEdits": [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 0
              },
              "end": {
                "line": 0,
                "character": 0
              }
            },
            "newText": "from inspect import walktree\n"
          }
        ]
      }
    ]
    "#);

    Ok(())
}

/// Tests that disabling auto-import works.
#[test]
fn disable_auto_import() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
walktr
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default().with_auto_import(false))
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let hints = server.completion_request(&server.file_uri(foo), Position::new(0, 6));

    insta::assert_json_snapshot!(hints, @"[]");

    Ok(())
}
