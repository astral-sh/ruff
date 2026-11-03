use anyhow::Result;
use lsp_types::{Position, Range};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn import_component_ranges_use_the_source_spelling() -> Result<()> {
    let file = SystemPath::new("src/main.py");
    let source = "import cafe\u{301}\nfrom cafe\u{301} . menu import special\n";
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file("src/café/__init__.py", "")?
        .with_file("src/café/menu.py", "special = 1\n")?
        .with_file(file, source)?
        .build()
        .wait_until_workspaces_are_initialized();
    server.open_text_document(file, source, 1);

    let tokens = server
        .semantic_tokens_full_request(&server.file_uri(file))
        .expect("expected semantic tokens for the imports");
    let namespaces: Vec<_> = tokens
        .data
        .iter()
        .filter(|token| token.token_type == ty_ide::SemanticTokenType::Namespace as u32)
        .map(|token| (token.delta_line, token.delta_start, token.length))
        .collect();
    assert_eq!(namespaces, [(0, 7, 5), (1, 5, 5), (0, 8, 4)]);

    for (line, start, end) in [(0, 7, 12), (1, 13, 17)] {
        let hover = server
            .hover_request(file, Position::new(line, start + 1))
            .expect("expected hover for the imported module");
        assert_eq!(
            hover.range,
            Some(Range::new(
                Position::new(line, start),
                Position::new(line, end)
            ))
        );
    }
    Ok(())
}

#[test]
fn script_metadata_is_highlighted_as_toml() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let script = SystemPath::new("src/script.py");
    let source = r#"#!/usr/bin/env python3
# /// script
# dependencies = ["httpx"]
# requires-python = ">=3.12"
# ///
value = 1
"#;

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(script, source)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(script, source, 1);

    let tokens = server
        .semantic_tokens_full_request(&server.file_uri(script))
        .ok_or_else(|| anyhow::anyhow!("expected semantic tokens for the script"))?;

    let actual: Vec<_> = tokens
        .data
        .into_iter()
        .map(|token| {
            (
                token.delta_line,
                token.delta_start,
                token.length,
                token.token_type,
            )
        })
        .collect();

    assert_eq!(
        actual,
        vec![
            (2, 2, 12, ty_ide::SemanticTokenType::Variable as u32),
            (0, 13, 1, ty_ide::SemanticTokenType::Operator as u32),
            (0, 2, 1, ty_ide::SemanticTokenType::Operator as u32),
            (0, 1, 7, ty_ide::SemanticTokenType::String as u32),
            (0, 7, 1, ty_ide::SemanticTokenType::Operator as u32),
            (1, 2, 15, ty_ide::SemanticTokenType::Variable as u32),
            (0, 16, 1, ty_ide::SemanticTokenType::Operator as u32),
            (0, 2, 8, ty_ide::SemanticTokenType::String as u32),
            (2, 0, 5, ty_ide::SemanticTokenType::Variable as u32),
            (0, 8, 1, ty_ide::SemanticTokenType::Number as u32),
        ]
    );

    Ok(())
}

#[test]
fn multiline_token_client_not_supporting_multiline_tokens() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"def my_function(param1: int, param2: str) -> bool:
    """Example function with PEP 484 type annotations.

    Args:
        param1: The first parameter.
        param2: The second parameter.

    Returns:
        The return value. True for success, False otherwise.

    """
"#;

    let mut server = TestServerBuilder::new()?
        .enable_multiline_token_support(false)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let tokens = server.semantic_tokens_full_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(tokens);

    Ok(())
}

#[test]
fn multiline_token_client_supporting_multiline_tokens() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"def my_function(param1: int, param2: str) -> bool:
    """Example function with PEP 484 type annotations.

    Args:
        param1: The first parameter.
        param2: The second parameter.

    Returns:
        The return value. True for success, False otherwise.

    """
"#;

    let mut server = TestServerBuilder::new()?
        .enable_multiline_token_support(true)
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let tokens = server.semantic_tokens_full_request(&server.file_uri(foo));

    insta::assert_json_snapshot!(tokens);

    Ok(())
}

// Regression test for https://github.com/astral-sh/ty/issues/2346
#[test]
fn no_stale_tokens_after_opening_the_same_file_with_new_content() -> Result<()> {
    let file_name = "src/foo";
    let initial_content =
        "def calculate_sum(a):\n # Version A: Basic math\n return a\n\nresult = calculate_sum(5)\n";
    let mut server = TestServerBuilder::new()?
        .enable_multiline_token_support(true)
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(file_name, initial_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(file_name, initial_content, 0);

    let initial_tokens = server
        .semantic_tokens_full_request(&server.file_uri(file_name))
        .unwrap();

    server.close_text_document(file_name);

    server.open_text_document(
        file_name,
        "# Version B: Basic greeting\ndef say_hello():\n print(\"Hello, World!\")\n\nsay_hello()\n",
        0,
    );

    let new_tokens = server
        .semantic_tokens_full_request(&server.file_uri(file_name))
        .unwrap();

    assert_ne!(initial_tokens, new_tokens);

    Ok(())
}
