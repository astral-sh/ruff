use std::time::Duration;

use anyhow::Result;
use lsp_types::{FileChangeType, FileEvent, notification::PublishDiagnostics};
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
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnostics>();

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn on_did_change_watched_files() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    print(a)
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, "")?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let foo = server.file_path(foo);

    server.open_text_document(&foo, "", 1);

    let _open_diagnostics = server.await_notification::<PublishDiagnostics>();

    std::fs::write(&foo, foo_content)?;

    server.did_change_watched_files(vec![FileEvent {
        uri: server.file_uri(foo),
        typ: FileChangeType::CHANGED,
    }]);

    let diagnostics = server.await_notification::<PublishDiagnostics>();

    // Note how ty reports no diagnostics here. This is because
    // the contents received by didOpen/didChange take precedence over the file
    // content on disk. Or, more specifically, because the revision
    // of the file is not bumped, because it still uses the version
    // from the `didOpen` notification but we don't have any notification
    // that we can use here.
    insta::assert_json_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn on_did_change_watched_files_pull_diagnostics() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    print(a)
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, "")?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    let foo = server.file_path(foo);

    server.open_text_document(&foo, "", 1);

    std::fs::write(&foo, foo_content)?;

    server.did_change_watched_files(vec![FileEvent {
        uri: server.file_uri(foo),
        typ: FileChangeType::CHANGED,
    }]);

    let diagnostics =
        server.try_await_notification::<PublishDiagnostics>(Some(Duration::from_millis(100)));

    assert!(
        diagnostics.is_err(),
        "Server should not send a publish diagnostic notification if the client supports pull diagnostics"
    );

    Ok(())
}
