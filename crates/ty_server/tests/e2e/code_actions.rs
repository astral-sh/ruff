use anyhow::Result;
use lsp_types::{Position, Range, request::CodeActionRequest};
use ruff_db::system::SystemPath;

use crate::TestServerBuilder;

#[test]
fn code_action() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 20 / 2  # ty: ignore[division-by-zero]
";

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "\
[rules]
unused-ignore-comment = \"warn\"
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, &foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);

    // Get code actions for the line with the unused ignore comment.
    let code_action_params = lsp_types::CodeActionParams {
        text_document: lsp_types::TextDocumentIdentifier {
            uri: server.file_uri(foo),
        },
        range: Range::new(Position::new(0, 0), Position::new(0, 43)),
        context: lsp_types::CodeActionContext {
            diagnostics: match diagnostics {
                lsp_types::DocumentDiagnosticReportResult::Report(
                    lsp_types::DocumentDiagnosticReport::Full(report),
                ) => report.full_document_diagnostic_report.items,
                _ => panic!("Expected full diagnostic report"),
            },
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };

    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}

#[test]
fn no_code_action_for_non_overlapping_range_on_same_line() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x = 20 / 2  # ty: ignore[division-by-zero]
";

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "\
[rules]
unused-ignore-comment = \"warn\"
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, &foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);

    // Get code actions for a range that doesn't overlap with the diagnostic.
    // The diagnostic is at characters 12-42, so we request actions for characters 0-10.
    let code_action_params = lsp_types::CodeActionParams {
        text_document: lsp_types::TextDocumentIdentifier {
            uri: server.file_uri(foo),
        },
        range: Range::new(Position::new(0, 0), Position::new(0, 10)),
        context: lsp_types::CodeActionContext {
            diagnostics: match diagnostics {
                lsp_types::DocumentDiagnosticReportResult::Report(
                    lsp_types::DocumentDiagnosticReport::Full(report),
                ) => report.full_document_diagnostic_report.items,
                _ => panic!("Expected full diagnostic report"),
            },
            only: None,
            trigger_kind: None,
        },
        work_done_progress_params: lsp_types::WorkDoneProgressParams::default(),
        partial_result_params: lsp_types::PartialResultParams::default(),
    };

    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    // Should return None because the range doesn't overlap with the diagnostic.
    assert_eq!(code_actions, None);

    Ok(())
}
