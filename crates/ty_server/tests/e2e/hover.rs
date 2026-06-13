use anyhow::Result;
use lsp_types::{Contents, MarkupKind, Position};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn hover_prefers_markdown_over_plain_text() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .with_hover_content_format(vec![MarkupKind::Markdown, MarkupKind::PlainText])
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server
        .hover_request(foo, Position::new(0, 0))
        .expect("Expected a hover response");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);

    Ok(())
}

#[test]
fn hover_prefers_plain_text_over_markdown() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .with_hover_content_format(vec![MarkupKind::PlainText, MarkupKind::Markdown])
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server
        .hover_request(foo, Position::new(0, 0))
        .expect("Expected a hover response");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::PlainText);

    Ok(())
}

#[test]
fn hover_markdown_only() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .with_hover_content_format(vec![MarkupKind::Markdown])
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server
        .hover_request(foo, Position::new(0, 0))
        .expect("Expected a hover response");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);

    Ok(())
}

#[test]
fn hover_plain_text_only() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .with_hover_content_format(vec![MarkupKind::PlainText])
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server
        .hover_request(foo, Position::new(0, 0))
        .expect("Expected a hover response");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::PlainText);

    Ok(())
}
