use anyhow::Result;
use insta::assert_json_snapshot;
use lsp_types::{Position, Range};

use crate::TestServerBuilder;

const CUSTOM_EXTENSION_CONFIG: &str = r#"[tool.ruff]
preview = true
extension = { thing = "markdown" }

[tool.ruff.format]
preview = true
"#;

const CUSTOM_EXTENSION_MARKDOWN: &str = "# title\n\n```python\nx='hi'\n```\n";

#[test]
fn format_custom_extension_mapped_to_markdown() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", CUSTOM_EXTENSION_CONFIG)?
        .build();

    server.open_text_document("test.thing", CUSTOM_EXTENSION_MARKDOWN, 1);

    let edits = server.format_request("test.thing");

    assert_json_snapshot!(
        edits,
        @r#"
    [
      {
        "range": {
          "start": {
            "line": 3,
            "character": 0
          },
          "end": {
            "line": 4,
            "character": 0
          }
        },
        "newText": "x = \"hi\"\n"
      }
    ]
    "#
    );

    Ok(())
}

#[test]
fn range_format_custom_extension_mapped_to_markdown_is_unsupported() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", CUSTOM_EXTENSION_CONFIG)?
        .build();

    server.open_text_document("test.thing", CUSTOM_EXTENSION_MARKDOWN, 1);

    let edits = server.format_range_request(
        "test.thing",
        Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 3,
                character: 6,
            },
        },
    );

    assert_json_snapshot!(edits, @"null");

    Ok(())
}

#[test]
fn lint_custom_extension_mapped_to_markdown_emits_no_diagnostics() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(".")?
        .with_file("pyproject.toml", CUSTOM_EXTENSION_CONFIG)?
        .build();

    server.open_text_document("test.thing", CUSTOM_EXTENSION_MARKDOWN, 1);

    let diagnostics = server.document_diagnostic_request("test.thing", None);

    assert_json_snapshot!(
        diagnostics,
        @r#"
    {
      "kind": "full",
      "items": []
    }
    "#
    );

    Ok(())
}
