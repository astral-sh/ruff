use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Context, Result};
use lsp_types::{
    DidOpenTextDocumentNotification, DidOpenTextDocumentParams, FileChangeType, FileEvent,
    LanguageKind, Message, Position, PublishDiagnosticsNotification, Range,
    TextDocumentContentChangePartial, TextDocumentContentChangeWholeDocument, TextDocumentItem,
    Uri,
};
use ruff_db::system::SystemPath;
use ty_server::ClientOptions;

use crate::notebook::NotebookBuilder;
use crate::{TestServer, TestServerBuilder};

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
        .with_auto_import_completion_command()
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);
    let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    insta::assert_debug_snapshot!(diagnostics);

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
fn on_did_save_publishes_open_file_documents() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let lib = SystemPath::new("src/lib.py");
    let main = SystemPath::new("src/main.py");

    let lib_content = "x: str = ''\n";
    let main_content = "\
from typing import assert_type
from lib import x

assert_type(x, str)
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(lib, lib_content)?
        .with_file(main, main_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(lib, lib_content, 1);
    server.await_notification::<PublishDiagnosticsNotification>();

    server.open_text_document(main, main_content, 1);
    server.await_notification::<PublishDiagnosticsNotification>();

    let mut notebook = NotebookBuilder::virtual_file("src/notebook.ipynb");
    let notebook_import = notebook.add_python_cell("from lib import x\n");
    let notebook_main_content = "\
from typing import assert_type

assert_type(x, str)
";
    let notebook_main = notebook.add_python_cell_with_version(notebook_main_content, 1);
    notebook.open(&mut server);
    // Opening the notebook publishes diagnostics for both notebook cells:
    // `src/notebook.ipynb#0` and `src/notebook.ipynb#1`.
    server.collect_publish_diagnostic_notifications(2);

    server.change_text_document(
        lib,
        vec![
            lsp_types::TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument {
                    text: "x: int = 1\n".to_string(),
                },
            ),
        ],
        2,
    );
    // Drain the diagnostics for `src/lib.py` triggered by `textDocument/didChange`
    // before asserting on the diagnostics triggered by `textDocument/didSave`.
    server.await_notification::<PublishDiagnosticsNotification>();

    server.save_text_document(lib);

    // Saving `src/lib.py` publishes four diagnostic notifications:
    // - one for `src/lib.py`,
    // - one for `src/main.py`, and
    // - two for `src/notebook.ipynb`, one per notebook cell (`#0` and `#1`).
    let diagnostics = collect_publish_diagnostic_notifications_with_versions(&mut server, 4);
    assert_eq!(diagnostics[&notebook_import].version, Some(0));
    assert_eq!(diagnostics[&notebook_main].version, Some(1));
    let diagnostics = diagnostics
        .into_iter()
        .map(|(uri, diagnostics)| (uri, diagnostics.diagnostics))
        .collect::<BTreeMap<_, _>>();
    insta::assert_json_snapshot!(diagnostics);

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
    let foo_uri = server.file_uri(&foo);

    server.open_text_document(&foo, "", 1);

    let _open_diagnostics = server.await_notification::<PublishDiagnosticsNotification>();

    let mut notebook = NotebookBuilder::virtual_file("src/notebook.ipynb");
    let first_cell = notebook.add_python_cell("x = 1\n");
    let second_cell = notebook.add_python_cell("x\n");
    notebook.open(&mut server);
    server.collect_publish_diagnostic_notifications(2);

    std::fs::write(&foo, foo_content)?;

    server.did_change_watched_files(vec![FileEvent {
        uri: foo_uri.clone(),
        kind: FileChangeType::Changed,
    }]);

    let mut diagnostics = collect_publish_diagnostic_notifications_with_versions(&mut server, 3);
    assert_eq!(diagnostics[&first_cell].version, Some(0));
    assert_eq!(diagnostics[&second_cell].version, Some(0));

    let extra_diagnostics = server
        .try_await_notification::<PublishDiagnosticsNotification>(Some(Duration::from_millis(100)));
    assert!(
        extra_diagnostics.is_err(),
        "Server should publish diagnostics once per open document"
    );

    let diagnostics = diagnostics
        .remove(&foo_uri)
        .with_context(|| format!("Expected diagnostics for {foo_uri}"))?;

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

fn collect_publish_diagnostic_notifications_with_versions(
    server: &mut TestServer,
    count: usize,
) -> BTreeMap<Uri, lsp_types::PublishDiagnosticsParams> {
    let mut results = BTreeMap::new();

    for _ in 0..count {
        let diagnostics = server.await_notification::<PublishDiagnosticsNotification>();
        let uri = diagnostics.uri.clone();

        assert!(
            results.insert(uri.clone(), diagnostics).is_none(),
            "Received multiple publish diagnostic notifications for {uri}"
        );
    }

    results
}
