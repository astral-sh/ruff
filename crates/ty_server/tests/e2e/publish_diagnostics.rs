use std::time::Duration;

use anyhow::Result;
use lsp_types::{
    DidOpenTextDocumentParams, FileChangeType, FileEvent, TextDocumentItem,
    notification::{DidOpenTextDocument, PublishDiagnostics},
};
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
fn on_did_open_diagnostics_off() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_diagnostic_mode(ty_server::DiagnosticMode::Off)),
        )?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics =
        server.try_await_notification::<PublishDiagnostics>(Some(Duration::from_millis(100)));

    assert!(
        diagnostics.is_err(),
        "Server should not send a publish diagnostics notification when diagnostics are off"
    );

    Ok(())
}

#[test]
fn on_did_change() -> Result<()> {
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
    let _ = server.await_notification::<PublishDiagnostics>();

    let changes = vec![lsp_types::TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: "def foo() -> int: return 42".to_string(),
    }];

    server.change_text_document(foo, changes, 2);

    let diagnostics = server.await_notification::<PublishDiagnostics>();

    assert_eq!(diagnostics.version, Some(2));

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn on_did_change_diagnostics_off() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_diagnostic_mode(ty_server::DiagnosticMode::Off)),
        )?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let changes = vec![lsp_types::TextDocumentContentChangeEvent {
        range: None,
        range_length: None,
        text: "def foo() -> int: return 42".to_string(),
    }];

    server.change_text_document(foo, changes, 2);

    let diagnostics =
        server.try_await_notification::<PublishDiagnostics>(Some(Duration::from_millis(100)));

    assert!(
        diagnostics.is_err(),
        "Server should not send a publish diagnostics notification when diagnostics are off"
    );

    Ok(())
}

#[test]
fn message_without_related_information_support() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"
from typing import assert_type

assert_type("test", list[str])
"#;

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
fn message_with_related_information_support() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"
from typing import assert_type

assert_type("test", list[str])
"#;

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_diagnostic_related_information(true)
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

#[test]
fn on_did_open_file_without_extension_but_python_language() -> Result<()> {
    let foo = SystemPath::new("src/foo");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
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
fn changing_language_of_file_without_extension() -> Result<()> {
    let foo = SystemPath::new("src/foo");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnostics>();

    insta::assert_debug_snapshot!(diagnostics);

    server.close_text_document(foo);

    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: server.file_uri(foo),
            language_id: "text".to_string(),
            version: 1,
            text: foo_content.to_string(),
        },
    };
    server.send_notification::<DidOpenTextDocument>(params);
    let _close_diagnostics = server.await_notification::<PublishDiagnostics>();

    let diagnostics = server.await_notification::<PublishDiagnostics>();

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn invalid_syntax_with_syntax_errors_disabled() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo(
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(ClientOptions::default().with_show_syntax_errors(false)),
        )?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    let diagnostics = server.await_notification::<PublishDiagnostics>();

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}
