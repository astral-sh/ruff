use anyhow::Result;
use lsp_types::{Position, notification::PublishDiagnostics};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::TestServerBuilder;

/// Tests that we get signature help even when the cursor
/// is on the function name.
///
/// This is a regression test to ensure we don't accidentally
/// cause this case to stop working.
#[test]
fn works_in_function_name() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
import re
re.match('', '')
";

    let mut server = TestServerBuilder::new()?
        .with_initialization_options(ClientOptions::default())
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let signature_help = server.signature_help_request(&server.file_uri(foo), Position::new(1, 6));

    insta::assert_json_snapshot!(signature_help);

    Ok(())
}
