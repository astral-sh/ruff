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
fn structured_docstrings_use_html_list_indentation_when_supported() -> Result<()> {
    let markdown = structured_hover(Some(vec!["ul".to_string()]), Some(true))?;
    insta::assert_snapshot!(&markdown, @"
    ```python
    def documented(value: str) -> None
    ```
    ---
    Summary.

    ## Parameters
    <ul>

    **value**: `str`  
    The input value.

    </ul>
    ");

    for tag in ["UL", "Ul"] {
        assert_eq!(
            structured_hover(Some(vec![tag.to_string()]), Some(true))?,
            markdown
        );
    }
    Ok(())
}

#[test]
fn structured_docstrings_avoid_html_list_indentation_without_support() -> Result<()> {
    let without_allowed_tags = structured_hover(None, None)?;
    insta::assert_snapshot!(&without_allowed_tags, @"
    ```python
    def documented(value: str) -> None
    ```
    ---
    Summary.

    ## Parameters
    **value**: `str`  
    The input value.
    ");

    let unrelated_allowed_tag = structured_hover(Some(vec!["p".to_string()]), Some(true))?;
    assert_eq!(unrelated_allowed_tag, without_allowed_tags);

    let without_ty_capability = structured_hover(Some(vec!["ul".to_string()]), None)?;
    assert_eq!(without_ty_capability, without_allowed_tags);
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

fn structured_hover(
    allowed_tags: Option<Vec<String>>,
    bare_ul_indentation: Option<bool>,
) -> Result<String> {
    let workspace_root = SystemPath::new("src");
    let document_path = SystemPath::new("src/foo.py");
    let document_content = r#"def documented(value: str) -> None:
    """Summary.

    :param str value: The input value.
    """
    ...

documented("x")
"#;

    let mut builder = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(document_path, document_content)?
        .with_hover_content_format(vec![MarkupKind::Markdown]);
    if let Some(allowed_tags) = allowed_tags {
        builder = builder.with_markdown_allowed_tags(allowed_tags);
    }
    if let Some(supported) = bare_ul_indentation {
        builder = builder.with_ty_markdown_bare_ul_indentation(supported);
    }

    let mut server = builder.build().wait_until_workspaces_are_initialized();
    server.open_text_document(document_path, document_content, 1);

    let hover = server
        .hover_request(document_path, Position::new(7, 1))
        .expect("Expected a hover response");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("Expected markup content");
    };

    Ok(markup.value)
}
