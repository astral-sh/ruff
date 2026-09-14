//! Requests for disk-backed targets must not require client document ownership.

use lsp_types::{
    Contents, Definition, DefinitionResponse, DocumentDiagnosticReport, DocumentSymbolParams,
    DocumentSymbolRequest, DocumentSymbolResponse, FileChangeType, FileEvent, Position,
    TextDocumentContentChangeEvent, TextDocumentContentChangeWholeDocument, TextDocumentIdentifier,
    Uri,
};
use ruff_db::system::{SystemPath, SystemPathBuf};
use ty_server::{ClientOptions, DiagnosticMode};

use crate::{AwaitResponseError, TestServer, TestServerBuilder};

#[test]
fn workspace_files_without_open_notifications() {
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
    let mut builder = TestServerBuilder::new()
        .expect("test server")
        .with_file(
            "ty.toml",
            r#"[src]
exclude = ["excluded"]
"#,
        )
        .expect("configuration");
    for path in paths {
        builder = builder.with_file(path, source).expect("source");
    }
    let mut server = builder.build().wait_until_workspaces_are_initialized();
    for path in paths {
        let uri = server.file_uri(path);
        assert_eq!(symbols(&mut server, uri.clone()), ["answer"]);
        assert!(server.hover_request(path, Position::new(3, 2)).is_some());
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
    }
    assert!(
        server
            .rename(&server.file_uri("main.py"), Position::new(0, 5), "renamed")
            .expect("rename response")
            .is_some()
    );
}

#[test]
fn external_dependency_lifecycle_and_watched_changes() {
    let path = SystemPath::new("deps/library.py");
    let disk = "\
import sys
value = sys.platform
value
";
    let mut server = dependency_server(false, disk);
    assert_hover(&mut server, path, Position::new(2, 0), "Literal[\"win32\"]");
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
    let edited = "\
value = 'edited'
value
";
    server.change_text_document(
        path,
        vec![
            TextDocumentContentChangeEvent::TextDocumentContentChangeWholeDocument(
                TextDocumentContentChangeWholeDocument {
                    text: edited.into(),
                },
            ),
        ],
        2,
    );
    assert_hover(
        &mut server,
        path,
        Position::new(1, 0),
        "Literal[\"edited\"]",
    );
    server.close_text_document(path);
    assert_hover(&mut server, path, Position::new(2, 0), "Literal[\"win32\"]");
    server.write_file(path, edited).expect("disk edit");
    notify_change(&mut server, path, FileChangeType::Changed);
    assert_hover(
        &mut server,
        path,
        Position::new(1, 0),
        "Literal[\"edited\"]",
    );
    std::fs::remove_file(server.file_path(path)).expect("delete dependency");
    notify_change(&mut server, path, FileChangeType::Deleted);
    assert!(server.hover_request(path, Position::new(1, 0)).is_none());
    server.write_file(path, disk).expect("recreate dependency");
    notify_change(&mut server, path, FileChangeType::Created);
    assert_hover(&mut server, path, Position::new(2, 0), "Literal[\"win32\"]");
}

#[test]
fn closed_external_file_respects_project_editor_settings() {
    let mut server = dependency_server(
        true,
        "\
value = 42
value
",
    );
    assert!(
        server
            .hover_request("deps/library.py", Position::new(1, 0))
            .is_none()
    );
}

#[test]
fn closed_diagnostics_preserve_checking_policy() {
    for mode in [
        DiagnosticMode::Off,
        DiagnosticMode::OpenFilesOnly,
        DiagnosticMode::Workspace,
    ] {
        let source = "\
import os
undefined_name
";
        let mut server = TestServerBuilder::new()
            .expect("test server")
            .with_workspace(SystemPath::new("src"), None)
            .expect("workspace")
            .with_initialization_options(&ClientOptions::default().with_diagnostic_mode(mode))
            .with_file(
                "src/ty.toml",
                r#"[src]
exclude = ["excluded.py"]
[environment]
extra-paths = ["../deps"]
"#,
            )
            .expect("configuration")
            .with_files([
                ("src/main.py", source),
                ("src/excluded.py", source),
                ("deps/library.py", source),
            ])
            .expect("sources")
            .build()
            .wait_until_workspaces_are_initialized();
        for path in [
            "src/main.py",
            "src/excluded.py",
            "deps/library.py",
            "src/missing.py",
        ] {
            let report = server.document_diagnostic_request(path, None);
            let DocumentDiagnosticReport::RelatedFullDocumentDiagnosticReport(report) = report
            else {
                panic!("expected full diagnostics");
            };
            let items = report.full_document_diagnostic_report.items;
            if mode == DiagnosticMode::Workspace && path == "src/main.py" {
                assert!(
                    items.iter().any(|diagnostic| matches!(&diagnostic.message,
                        lsp_types::Message::String(message) if message.contains("undefined_name")))
                );
            } else {
                assert!(items.is_empty(), "{path}: {items:?}");
            }
        }
    }
}

#[test]
fn missing_files_and_unsupported_targets() {
    let mut server = TestServerBuilder::new()
        .expect("test server")
        .with_workspace(SystemPath::new("src"), None)
        .expect("workspace")
        .build()
        .wait_until_workspaces_are_initialized();
    assert!(
        server
            .send_request_await::<DocumentSymbolRequest>(symbol_params(
                server.file_uri("src/missing.py")
            ))
            .is_none()
    );
    assert!(server.hover_request("src", Position::new(0, 0)).is_none());
    for uri in [
        server.file_uri("outside.py"),
        server.file_uri("src/notebook.ipynb"),
        Uri::parse("untitled:unknown").expect("URI"),
    ] {
        let id = server.send_request::<DocumentSymbolRequest>(symbol_params(uri));
        assert!(
            matches!(server.try_await_response::<DocumentSymbolRequest>(&id, None),
            Err(AwaitResponseError::RequestFailed(error)) if error.code == lsp_server::ErrorCode::InvalidParams as i32)
        );
    }
}

#[test]
fn bundled_stub_can_be_queried_without_opening_it() {
    let mut server = TestServerBuilder::new()
        .expect("test server")
        .with_file(
            "main.py", "\
int
",
        )
        .expect("source")
        .build()
        .wait_until_workspaces_are_initialized();
    let definition = server
        .goto_definition_request("main.py", Position::new(0, 1))
        .expect("definition");
    let DefinitionResponse::Definition(Definition::LocationList(locations)) = definition else {
        panic!("expected definition locations");
    };
    let location = &locations[0];
    let uri = location.uri.clone();
    let path =
        SystemPathBuf::from_path_buf(uri.to_file_path().expect("file URI")).expect("UTF-8 path");
    assert!(
        symbols(&mut server, uri.clone())
            .iter()
            .any(|name| name == "int")
    );
    assert!(server.hover_request(&path, location.range.start).is_some());
    // The materialized stub also obeys normal client ownership while opened.
    server.open_text_document(
        &path,
        "\
overlay = 42
",
        1,
    );
    assert_eq!(symbols(&mut server, uri.clone()), ["overlay"]);
    server.close_text_document(&path);
    assert!(symbols(&mut server, uri).iter().any(|name| name == "int"));
}

#[test]
fn containing_projects_precede_search_paths_and_shared_paths_use_root_order() {
    let mut builder = TestServerBuilder::new().expect("test server");
    for (root, platform) in [("a", "win32"), ("a/nested", "linux"), ("b", "darwin")] {
        let parent = if root == "a/nested" { "../.." } else { ".." };
        builder = builder
            .with_workspace(SystemPath::new(root), None)
            .expect("workspace")
            .with_file(
                format!("{root}/ty.toml"),
                format!(
                    r#"[environment]
python-platform = "{platform}"
extra-paths = ["{parent}/deps", "{parent}/b"]
"#
                ),
            )
            .expect("configuration")
            .with_file(
                format!("{root}/main.py"),
                "\
import sys
sys.platform
",
            )
            .expect("source");
    }
    let mut server = builder
        .with_file(
            "deps/library.py",
            "\
import sys
sys.platform
",
        )
        .expect("dependency")
        .build()
        .wait_until_workspaces_are_initialized();
    for (path, expected) in [
        ("a/nested/main.py", "Literal[\"linux\"]"),
        ("b/main.py", "Literal[\"darwin\"]"),
        ("deps/library.py", "Literal[\"win32\"]"),
    ] {
        assert_hover(
            &mut server,
            SystemPath::new(path),
            Position::new(1, 5),
            expected,
        );
    }
}

fn dependency_server(disabled: bool, source: &str) -> TestServer {
    TestServerBuilder::new()
        .expect("test server")
        .with_workspace(SystemPath::new("a"), None)
        .expect("first workspace")
        .with_workspace(
            SystemPath::new("b/src"),
            Some(ClientOptions::default().with_disable_language_services(disabled)),
        )
        .expect("dependency workspace")
        .with_file(
            "b/ty.toml",
            r#"[environment]
extra-paths = ["../deps"]
python-platform = "win32"
"#,
        )
        .expect("configuration")
        .with_file("deps/library.py", source)
        .expect("dependency")
        .build()
        .wait_until_workspaces_are_initialized()
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

fn assert_hover(server: &mut TestServer, path: &SystemPath, position: Position, expected: &str) {
    let hover = server.hover_request(path, position).expect("hover");
    let Contents::MarkupContent(markup) = hover.contents else {
        panic!("expected markup");
    };
    assert_eq!(markup.value, expected);
}

fn notify_change(server: &mut TestServer, path: &SystemPath, kind: FileChangeType) {
    server.did_change_watched_files(vec![FileEvent {
        uri: server.file_uri(path),
        kind,
    }]);
}
