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
