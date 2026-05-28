use anyhow::Result;
use lsp_types::request::OnTypeFormatting;
use lsp_types::{
    DocumentOnTypeFormattingParams, FormattingOptions, Position, TextDocumentIdentifier,
    TextDocumentPositionParams,
};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn closes_triple_quoted_string() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "def foo():\n    \"\"\"\n    return 42\n\n\ndef bar():\n    \"\"\"Existing docstring.\"\"\"";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let edits = server.send_request_await::<OnTypeFormatting>(DocumentOnTypeFormattingParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier {
                uri: server.file_uri(foo),
            },
            position: Position::new(1, 7),
        },
        ch: "\"".to_string(),
        options: FormattingOptions::default(),
    });

    insta::assert_json_snapshot!(edits, @r#"
    [
      {
        "range": {
          "start": {
            "line": 1,
            "character": 7
          },
          "end": {
            "line": 1,
            "character": 7
          }
        },
        "newText": "\"\"\""
      }
    ]
    "#);

    Ok(())
}
