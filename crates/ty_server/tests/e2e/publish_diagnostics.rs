use std::time::Duration;

use anyhow::Result;
use lsp_types::{
    DidOpenTextDocumentNotification, DidOpenTextDocumentParams, FileChangeType, FileEvent,
    LanguageKind, Message, Position, PublishDiagnosticsNotification, Range,
    TextDocumentContentChangePartial, TextDocumentContentChangeWholeDocument, TextDocumentItem,
    Uri,
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
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn full_diagnostic_output() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .with_full_diagnostic_output()
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
    let rendered = diagnostics.diagnostics[0]
        .data
        .as_ref()
        .and_then(|data| data.get("rendered"))
        .and_then(serde_json::Value::as_str)
        .expect("diagnostic should include fully rendered output");
    let diagnostic_id = diagnostics.diagnostics[0]
        .data
        .as_ref()
        .and_then(|data| data.get("diagnostic_id"))
        .and_then(serde_json::Value::as_str);

    assert_eq!(diagnostic_id, Some("invalid-return-type"));
    assert!(rendered.contains("Return type does not match returned value"));
    assert!(rendered.contains("def foo() -> str:"));
    assert!(rendered.contains("return 42"));
    assert!(rendered.contains("invalid-return-type"));

    Ok(())
}

/// Tests that we get diagnostics for a file that is NOT saved to
/// disk when using `OpenFilesOnly` diagnostic mode.
#[test]
fn on_did_open_non_existing_file_open_files_only() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(
                ClientOptions::default()
                    .with_diagnostic_mode(ty_server::DiagnosticMode::OpenFilesOnly),
            ),
        )?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

/// Tests that we get diagnostics for a file that is NOT saved to disk when
/// using `Workspace` diagnostic mode.
///
/// Basically, ty currently doesn't know whether a `file://...` path refers
/// to a file that doesn't exist or not. To work around that, we always check
/// the open file set for whether we should "check" a file or not.
#[test]
fn on_did_open_non_existing_file_workspace_with_file_uri() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(
                ClientOptions::default().with_diagnostic_mode(ty_server::DiagnosticMode::Workspace),
            ),
        )?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

/// Like `on_did_open_non_existing_file_workspace_with_file_uri`, but uses
/// a `untitled://...` URI instead of `file://...`.
///
/// Notably, this makes diagnostics for opened files that aren't saved to
/// disk yet work without needing to check the open file set explicitly. It's
/// because ty follows the LSP protocol convention that URIs to files that
/// _don't_ use the `file` scheme refer to documents that aren't saved to disk
/// yet. So ty correctly detects this as a virtual file and returns diagnostics
/// for it.
///
/// Ref: <https://github.com/astral-sh/ruff/issues/15392>
/// Ref: <https://github.com/neovim/neovim/issues/21276>
/// Ref: <https://github.com/microsoft/language-server-protocol/issues/1030>
#[test]
fn on_did_open_non_existing_file_workspace_with_untitled_uri() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            Some(
                ClientOptions::default().with_diagnostic_mode(ty_server::DiagnosticMode::Workspace),
            ),
        )?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.send_notification::<DidOpenTextDocumentNotification>(DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: {
                let uri = server.file_uri(foo);
                Uri::parse(&format!("untitled://{}", uri.path())).unwrap()
            },
            language_id: LanguageKind::Python,
            version: 1,
            text: foo_content.to_string(),
        },
    });
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
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
    let diagnostics = server
        .try_await_notification::<PublishDiagnosticsNotification>(Some(Duration::from_millis(100)));

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
    let _ = server.await_notification::<PublishDiagnosticsNotification>();

    let changes = vec![
        lsp_types::TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
            TextDocumentContentChangeWholeDocument {
                text: "def foo() -> int: return 42".to_string(),
            },
        ),
    ];

    server.change_text_document(foo, changes, 2);

    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    assert_eq!(diagnostics.version, Some(2));

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn on_did_change_invalid_tuple_assignment_target_does_not_panic() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
something, somethingelse = (1, 2)
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnosticsNotification>();

    server.change_text_document(
        foo,
        vec![
            lsp_types::TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                TextDocumentContentChangePartial {
                    range: Range::new(Position::new(0, 11), Position::new(0, 24)),
                    text: "not".to_string(),
                    ..Default::default()
                },
            ),
        ],
        2,
    );

    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    assert_eq!(diagnostics.version, Some(2));

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn on_did_change_nested_invalid_tuple_assignment_target_does_not_panic() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
something, somethingelse = (1, 2)
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let _ = server.await_notification::<PublishDiagnosticsNotification>();

    server.change_text_document(
        foo,
        vec![
            lsp_types::TextDocumentContentChangeEvent::TextDocumentContentChangePartial(
                TextDocumentContentChangePartial {
                    range: Range::new(Position::new(0, 11), Position::new(0, 24)),
                    text: "not x".to_string(),
                    ..Default::default()
                },
            ),
        ],
        2,
    );

    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

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

    let changes = vec![
        lsp_types::TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
            TextDocumentContentChangeWholeDocument {
                text: "def foo() -> int: return 42".to_string(),
            },
        ),
    ];

    server.change_text_document(foo, changes, 2);

    let diagnostics = server
        .try_await_notification::<PublishDiagnosticsNotification>(Some(Duration::from_millis(100)));

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
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

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
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

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

    let _open_diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    std::fs::write(&foo, foo_content)?;

    server.did_change_watched_files(vec![FileEvent {
        uri: server.file_uri(foo),
        kind: FileChangeType::Changed,
    }]);

    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

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
        kind: FileChangeType::Changed,
    }]);

    let diagnostics = server
        .try_await_notification::<PublishDiagnosticsNotification>(Some(Duration::from_millis(100)));

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
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

/// `JupyterLab` presents notebook virtual documents to language servers as simple text files:
/// <https://github.com/jupyterlab/jupyterlab/blob/f51404192bf6d0ff79187c884f21e1f91b928146/packages/lsp/src/virtual/document.ts#L308-L314>
#[test]
fn on_did_open_ipynb_file_with_python_language() -> Result<()> {
    let foo = SystemPath::new("src/foo.ipynb");
    let foo_content = "\
def foo() -> str:
    return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
    let [diagnostic] = diagnostics.diagnostics.as_slice() else {
        panic!("expected one diagnostic, got {diagnostics:#?}");
    };
    let Message::String(message) = &diagnostic.message else {
        panic!(
            "expected string-type diagnostic message, got {:#?}",
            diagnostic.message
        );
    };

    insta::assert_snapshot!(
        message,
        @"Return type does not match returned value: expected `str`, found `Literal[42]`"
    );

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
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    insta::assert_debug_snapshot!(diagnostics);

    server.close_text_document(foo);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
    insta::assert_debug_snapshot!(diagnostics);

    let params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: server.file_uri(foo),
            language_id: LanguageKind::new("text"),
            version: 1,
            text: foo_content.to_string(),
        },
    };
    server.send_notification::<DidOpenTextDocumentNotification>(params);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

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

    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}
