use anyhow::Result;
use lsp_types::{Contents, MarkupKind, Position};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn prefers_markdown_when_listed_first() -> Result<()> {
    assert_eq!(
        hover_content_format(vec![MarkupKind::Markdown, MarkupKind::PlainText])?,
        MarkupKind::Markdown,
    );
    Ok(())
}

#[test]
fn prefers_plain_text_when_listed_first() -> Result<()> {
    assert_eq!(
        hover_content_format(vec![MarkupKind::PlainText, MarkupKind::Markdown])?,
        MarkupKind::PlainText,
    );
    Ok(())
}

#[test]
fn supports_only_markdown() -> Result<()> {
    assert_eq!(
        hover_content_format(vec![MarkupKind::Markdown])?,
        MarkupKind::Markdown
    );
    Ok(())
}

#[test]
fn supports_only_plain_text() -> Result<()> {
    assert_eq!(
        hover_content_format(vec![MarkupKind::PlainText])?,
        MarkupKind::PlainText
    );
    Ok(())
}

#[test]
fn shared_import_hover_uses_each_script_python_version() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let shared = SystemPath::new("src/shared.py");
    let older = SystemPath::new("src/older.py");
    let newer = SystemPath::new("src/newer.py");
    let shared_content = "\
import sys

if sys.version_info >= (3, 13):
    value = 13
else:
    value = 12
";
    let older_content = r#"# /// script
# requires-python = ">=3.12"
# [tool.ty.environment]
# extra-paths = ["."]
# ///

from shared import value
value
"#;
    let newer_content = r#"# /// script
# requires-python = ">=3.13"
# [tool.ty.environment]
# extra-paths = ["."]
# ///

from shared import value
value
"#;

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(
            "src/pyproject.toml",
            r#"[tool.ty.environment]
python-version = "3.12"
"#,
        )?
        .with_file(shared, shared_content)?
        .with_file(older, older_content)?
        .with_file(newer, newer_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(older, older_content, 1);
    server.open_text_document(newer, newer_content, 1);

    let older_hover = server.hover_request(older, Position::new(7, 1));
    insta::assert_json_snapshot!(older_hover, @r#"
    {
      "contents": {
        "kind": "plaintext",
        "value": "Literal[12]"
      },
      "range": {
        "start": {
          "line": 7,
          "character": 0
        },
        "end": {
          "line": 7,
          "character": 5
        }
      }
    }
    "#);

    let newer_hover = server.hover_request(newer, Position::new(7, 1));
    insta::assert_json_snapshot!(newer_hover, @r#"
    {
      "contents": {
        "kind": "plaintext",
        "value": "Literal[13]"
      },
      "range": {
        "start": {
          "line": 7,
          "character": 0
        },
        "end": {
          "line": 7,
          "character": 5
        }
      }
    }
    "#);

    Ok(())
}

fn hover_content_format(formats: Vec<MarkupKind>) -> Result<MarkupKind> {
    let workspace_root = SystemPath::new("src");
    let document_path = SystemPath::new("src/foo.py");
    let document_content = "\
    x: int = 1
    ";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(document_path, document_content)?
        .with_hover_content_format(formats)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(document_path, document_content, 1);

    let hover = server
        .hover_request(document_path, Position::new(0, 0))
        .expect("Expected a hover response");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    Ok(markup.kind)
}
