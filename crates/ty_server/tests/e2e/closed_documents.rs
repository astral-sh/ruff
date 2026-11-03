//! Requests for disk-backed targets must not require client document ownership.

use anyhow::Result;
use insta::assert_snapshot;
use lsp_types::{
    Contents, Diagnostic, DocumentDiagnosticReport, DocumentSymbolParams, DocumentSymbolRequest,
    DocumentSymbolResponse, FileChangeType, Position, TextDocumentIdentifier, Uri,
};
use ruff_db::system::SystemPath;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::diagnostic_snapshots::condensed_document_diagnostic_snapshot;
use crate::{AwaitResponseError, TestServer, TestServerBuilder};

#[test]
fn server_fulfills_requests_for_closed_workspace_files() -> Result<()> {
    let source = "\
def answer() -> int:
    return 42

answer()
";
    // No workspace folders: the server uses its current directory.
    let paths = [
        "main.py",
        "stub.pyi",
        "excluded/file.py",
        "script",
        "script.custom",
    ];
    let mut server = TestServerBuilder::new()?
        .with_file(
            "ty.toml",
            r#"[src]
exclude = ["excluded"]
"#,
        )?
        .with_files(paths.map(|path| (path, source)))?
        .build()
        .wait_until_workspaces_are_initialized();

    // Test that we support requests against every type of file on disk.
    for path in paths {
        let uri = server.file_uri(path);
        assert_eq!(symbols(&mut server, uri), ["answer"]);
    }

    let uri = server.file_uri("main.py");

    // Test a variety of request types that exercise different aspects of the language server
    // (but, for efficiency, only do this part on a single file).
    assert!(
        server
            .hover_request("main.py", Position::new(3, 2))
            .is_some()
    );
    assert!(
        !server
            .semantic_tokens_full_request(&uri)
            .expect("tokens")
            .data
            .is_empty()
    );
    assert!(
        !server
            .folding_range_request(&uri)
            .expect("folds")
            .is_empty()
    );
    assert!(
        server
            .rename(&uri, Position::new(0, 5), "renamed")
            .expect("rename response")
            .is_some()
    );

    Ok(())
}

#[test]
fn opening_extensionless_script_enables_inline_settings() -> Result<()> {
    // The workspace and this extensionless script configure different import search paths, each
    // containing a `dependency` module. While closed, the script isn't indexed as a project file,
    // so its inline settings are ignored and hovering over `value` shows `Literal["workspace"]`.
    // Opening the same contents as Python enables the inline settings, changing the hover to
    // `Literal["script"]`. We accept different results for identical contents before and after
    // opening because requests for closed files are best-effort.

    let path = SystemPath::new("src/script");
    let source = r#"# /// script
# [tool.ty.environment]
# extra-paths = ["script-deps"]
# ///
from dependency import value
value
"#;
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_files([
            (
                "src/ty.toml",
                r#"environment.extra-paths = ["workspace-deps"]"#,
            ),
            ("src/workspace-deps/dependency.py", "value = 'workspace'"),
            ("src/script-deps/dependency.py", "value = 'script'"),
            (path.as_str(), source),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    let position = Position::new(5, 0);
    assert_hover(&mut server, path, position, "Literal[\"workspace\"]");

    server.open_text_document(path, source, 1);
    assert_hover(&mut server, path, position, "Literal[\"script\"]");

    Ok(())
}

#[test]
fn server_uses_current_contents_for_open_and_closed_files() -> Result<()> {
    let path = SystemPath::new("deps/library.py");
    let source = "\
value = 'disk'
value
";
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("a"), None)?
        .with_workspace(SystemPath::new("b/src"), None)?
        .with_files([
            ("b/ty.toml", r#"environment.extra-paths = ["../deps"]"#),
            (path.as_str(), source),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();
    assert_hover(&mut server, path, Position::new(1, 0), "Literal[\"disk\"]");

    // Opening the file must replace the cached disk contents with unsaved editor contents.
    let unsaved = "\
value = 'unsaved'
value
";
    server.open_text_document(path, unsaved, 1);
    assert_hover(
        &mut server,
        path,
        Position::new(1, 0),
        "Literal[\"unsaved\"]",
    );
    // Closing without saving must restore the disk contents.
    server.close_text_document(path);
    assert_hover(&mut server, path, Position::new(1, 0), "Literal[\"disk\"]");

    // While the file is closed, filesystem notifications must refresh the cached contents.
    let edited = "\
value = 'edited'
value
";
    server.write_file(path, edited)?;
    server.did_change_watched_file(path, FileChangeType::Changed);
    assert_hover(
        &mut server,
        path,
        Position::new(1, 0),
        "Literal[\"edited\"]",
    );
    std::fs::remove_file(server.file_path(path))?;
    server.did_change_watched_file(path, FileChangeType::Deleted);
    assert!(server.hover_request(path, Position::new(1, 0)).is_none());
    server.write_file(path, source)?;
    server.did_change_watched_file(path, FileChangeType::Created);
    assert_hover(&mut server, path, Position::new(1, 0), "Literal[\"disk\"]");

    Ok(())
}

#[test]
fn closed_unrelated_file_uses_fallback_project() -> Result<()> {
    let path = SystemPath::new("outside.py");
    let source = "\
from project_value import value
value
";
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("b"), None)?
        .with_workspace(SystemPath::new("a"), None)?
        .with_files([
            ("a/project_value.py", "value = 'a'"),
            ("b/project_value.py", "value = 'b'"),
            (path.as_str(), source),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    // Neither project contains the file; the first workspace by path supplies its environment.
    assert_hover(&mut server, path, Position::new(1, 0), "Literal[\"a\"]");

    Ok(())
}

#[test]
fn closed_external_file_respects_project_editor_settings() -> Result<()> {
    let source = "\
value = 42
value
";
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("a"), None)?
        .with_workspace(
            SystemPath::new("b/src"),
            Some(ClientOptions::default().with_disable_language_services(true)),
        )?
        .with_files([
            ("b/ty.toml", r#"environment.extra-paths = ["../deps"]"#),
            ("deps/library.py", source),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();
    assert!(
        server
            .hover_request("deps/library.py", Position::new(1, 0))
            .is_none()
    );

    Ok(())
}

#[test]
fn server_returns_no_closed_file_diagnostics_when_disabled() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_initialization_options(
            &ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Off),
        )
        .with_file("src/main.py", "undefined_name")?
        .build()
        .wait_until_workspaces_are_initialized();

    // Disabling diagnostics also suppresses results for explicitly requested closed files.
    assert_eq!(document_diagnostics(&mut server, "src/main.py"), []);

    Ok(())
}

#[test]
fn server_returns_no_closed_file_diagnostics_in_open_files_mode() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_initialization_options(
            &ClientOptions::default().with_diagnostic_mode(DiagnosticMode::OpenFilesOnly),
        )
        .with_file("src/main.py", "undefined_name")?
        .build()
        .wait_until_workspaces_are_initialized();

    // An explicit request must not make a closed file count as open.
    assert_eq!(document_diagnostics(&mut server, "src/main.py"), []);

    Ok(())
}

#[test]
fn server_reports_only_included_closed_files_in_workspace_mode() -> Result<()> {
    let source = "\
import os
undefined_name
";
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_initialization_options(
            &ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )
        .with_files([
            (
                "src/ty.toml",
                r#"[src]
exclude = ["excluded.py"]
[environment]
extra-paths = ["../deps"]
"#,
            ),
            ("src/main.py", source),
            ("src/excluded.py", source),
            ("deps/library.py", source),
        ])?
        .build()
        .wait_until_workspaces_are_initialized();

    // Workspace mode checks included files even if the client has never opened them.
    assert_snapshot!(
        condensed_document_diagnostic_snapshot(server.document_diagnostic_request("src/main.py", None)),
        @"1:0..1:14[ERROR]: Name `undefined_name` used when not defined"
    );

    // Explicit requests still respect exclusions from type checking.
    assert_eq!(document_diagnostics(&mut server, "src/excluded.py"), []);

    // Import search paths allow analysis of external files without enabling their diagnostics.
    assert_eq!(document_diagnostics(&mut server, "deps/library.py"), []);

    // Missing files produce empty reports rather than request errors.
    assert_eq!(document_diagnostics(&mut server, "src/missing.py"), []);

    Ok(())
}

#[test]
fn server_rejects_unsupported_targets() -> Result<()> {
    let mut server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(
            "src/notebook.ipynb",
            r#"{"cells": [], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"#,
        )?
        .build()
        .wait_until_workspaces_are_initialized();

    // Closed notebooks lack the cell URIs and position mappings supplied by the client.
    let uri = server.file_uri("src/notebook.ipynb");
    let id = server.send_request::<DocumentSymbolRequest>(symbol_params(uri.clone()));
    let response = server.try_await_response::<DocumentSymbolRequest>(&id, None);
    let Err(AwaitResponseError::RequestFailed(error)) = response else {
        anyhow::bail!("expected a request error, got {response:?}");
    };
    assert_eq!(error.code, lsp_server::ErrorCode::InvalidParams as i32);
    assert_eq!(
        error.message,
        format!("Document {uri} is neither open nor a supported closed file")
    );

    // An unopened virtual document has neither disk contents nor client-provided contents.
    let uri = Uri::parse("untitled:unknown")?;
    let id = server.send_request::<DocumentSymbolRequest>(symbol_params(uri.clone()));
    let response = server.try_await_response::<DocumentSymbolRequest>(&id, None);
    let Err(AwaitResponseError::RequestFailed(error)) = response else {
        anyhow::bail!("expected a request error, got {response:?}");
    };
    assert_eq!(error.code, lsp_server::ErrorCode::InvalidParams as i32);
    assert_eq!(
        error.message,
        format!("Document {uri} is neither open nor a supported closed file")
    );

    Ok(())
}

#[track_caller]
fn document_diagnostics(server: &mut TestServer, path: &str) -> Vec<Diagnostic> {
    let report = server.document_diagnostic_request(path, None);
    let DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) = report else {
        panic!("expected full diagnostics for {path}");
    };
    report.full_document_diagnostic_report.items
}

fn symbol_params(uri: Uri) -> DocumentSymbolParams {
    DocumentSymbolParams {
        text_document: TextDocumentIdentifier { uri },
        work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    }
}

fn symbols(server: &mut TestServer, uri: Uri) -> Vec<String> {
    match server
        .send_request_await::<DocumentSymbolRequest>(symbol_params(uri))
        .expect("symbols")
    {
        DocumentSymbolResponse::SymbolInformationList(symbols) => symbols
            .into_iter()
            .map(|symbol| symbol.base_symbol_information.name)
            .collect(),
        DocumentSymbolResponse::DocumentSymbolList(symbols) => {
            symbols.into_iter().map(|symbol| symbol.name).collect()
        }
    }
}

#[track_caller]
fn assert_hover(server: &mut TestServer, path: &SystemPath, position: Position, expected: &str) {
    let hover = server.hover_request(path, position).expect("hover");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("expected markup");
    };
    assert_eq!(markup.value, expected);
}
