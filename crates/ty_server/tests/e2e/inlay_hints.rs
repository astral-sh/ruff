use anyhow::Result;
use lsp_types::{Position, Range, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

/// Tests that the default value of inlay hints settings is correct i.e., they're all enabled
/// by default.
#[test]
fn default_inlay_hints() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 1

def foo(a: int) -> int:
    return a + 1

foo(1)
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_inlay_hints(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    let hints = server
        .inlay_hints_request(foo, Range::new(Position::new(0, 0), Position::new(6, 0)))?
        .unwrap();

    insta::assert_json_snapshot!(hints, @r#"
    [
      {
        "position": {
          "line": 0,
          "character": 1
        },
        "label": [
          {
            "value": ": "
          },
          {
            "value": "Literal[1]"
          }
        ],
        "kind": 1
      },
      {
        "position": {
          "line": 5,
          "character": 4
        },
        "label": [
          {
            "value": "a"
          },
          {
            "value": "="
          }
        ],
        "kind": 2
      }
    ]
    "#);

    Ok(())
}

/// Tests that disabling variable types inlay hints works correctly.
#[test]
fn variable_inlay_hints_disabled() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "x = 1";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(
            ClientOptions::default().with_variable_types_inlay_hints(false),
        )
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_inlay_hints(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>()?;

    let hints = server
        .inlay_hints_request(foo, Range::new(Position::new(0, 0), Position::new(0, 5)))?
        .unwrap();

    assert!(
        hints.is_empty(),
        "Expected no inlay hints, but found: {hints:?}"
    );

    Ok(())
}
