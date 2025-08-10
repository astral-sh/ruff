use anyhow::Result;
use lsp_types::notification::PublishDiagnostics;
use ruff_db::system::SystemPath;

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
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo, &foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnostics>()?;

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}
