use anyhow::Result;
use lsp_types::{Position, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

#[test]
fn completions() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "type";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_completions(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, &foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let hints = server
        .completions_request(foo, Position::new(0, 4))
        .unwrap();

    insta::assert_json_snapshot!(hints, @r#"
    {
      "isIncomplete": true,
      "items": [
        {
          "label": "TypeError",
          "kind": 7,
          "detail": "<class 'TypeError'>",
          "documentation": {
            "kind": "plaintext",
            "value": "Inappropriate argument type.\n"
          },
          "sortText": "0"
        },
        {
          "label": "type",
          "kind": 7,
          "detail": "<class 'type'>",
          "documentation": {
            "kind": "plaintext",
            "value": "type(object) -> the object's type/ntype(name, bases, dict, **kwds) -> a new type\n"
          },
          "sortText": "1"
        }
      ]
    }
    "#);

    Ok(())
}
