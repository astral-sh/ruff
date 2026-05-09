use anyhow::Result;
use lsp_types::{MarkupKind, Position};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

/// Tests that hover returns markdown when the client prefers markdown.
#[test]
fn hover_prefers_markdown() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_hover_content_format(vec![MarkupKind::Markdown, MarkupKind::PlainText])
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server.hover_request(foo, Position::new(0, 0));

    let hover = hover.expect("Expected a hover response");
    let lsp_types::HoverContents::Markup(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::Markdown);

    Ok(())
}

/// Tests that hover returns plaintext when the client prefers plaintext,
/// even if the client also supports markdown.
///
/// This is the bug reported in <https://github.com/astral-sh/ty/issues/3366>:
/// the server was checking whether the `contentFormat` list *contains* markdown,
/// rather than respecting the preference order (first element = most preferred).
#[test]
fn hover_prefers_plaintext_over_markdown() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_hover_content_format(vec![MarkupKind::PlainText, MarkupKind::Markdown])
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server.hover_request(foo, Position::new(0, 0));

    let hover = hover.expect("Expected a hover response");
    let lsp_types::HoverContents::Markup(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::PlainText);

    Ok(())
}

/// Tests that hover returns plaintext when the client only supports plaintext.
#[test]
fn hover_plaintext_only() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: int = 1
";

    let mut server = TestServerBuilder::new()?
        .with_hover_content_format(vec![MarkupKind::PlainText])
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let hover = server.hover_request(foo, Position::new(0, 0));

    let hover = hover.expect("Expected a hover response");
    let lsp_types::HoverContents::Markup(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    assert_eq!(markup.kind, MarkupKind::PlainText);

    Ok(())
}
