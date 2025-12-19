use anyhow::Result;
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

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
        .enable_pull_diagnostics(true)
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
        .enable_pull_diagnostics(true)
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
