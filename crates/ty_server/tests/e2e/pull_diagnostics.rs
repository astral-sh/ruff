use anyhow::Result;
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

#[test]
fn on_did_open() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, ClientOptions::default())?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo, &foo_content, 1);
    let diagnostics = server.document_diagnostic_request(foo)?;

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}
