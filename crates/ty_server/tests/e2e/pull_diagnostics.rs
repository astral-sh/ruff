use anyhow::Result;
use lsp_types::{
    PreviousResultId, WorkspaceDiagnosticReportResult, WorkspaceDocumentDiagnosticReport,
};
use ruff_db::system::SystemPath;
use ty_server::{ClientOptions, DiagnosticMode};

use crate::{TestServer, TestServerBuilder};

#[test]
fn on_did_open() -> Result<()> {
    let _filter = filter_result_id();

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
    let diagnostics = server.document_diagnostic_request(foo, None)?;

    insta::assert_debug_snapshot!(diagnostics);

    Ok(())
}

#[test]
fn document_diagnostic_caching_unchanged() -> Result<()> {
    let _filter = filter_result_id();

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

    // First request with no previous result ID
    let first_response = server.document_diagnostic_request(foo, None)?;

    // Extract result ID from first response
    let result_id = match &first_response {
        lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(report),
        ) => report
            .full_document_diagnostic_report
            .result_id
            .as_ref()
            .expect("First response should have a result ID")
            .clone(),
        _ => panic!("First response should be a full report"),
    };

    // Second request with the previous result ID - should return Unchanged
    let second_response = server.document_diagnostic_request(foo, Some(result_id))?;

    // Verify it's an unchanged report
    match second_response {
        lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Unchanged(_),
        ) => {
            // Success - got unchanged report as expected
        }
        _ => panic!("Expected an unchanged report when diagnostics haven't changed"),
    }

    Ok(())
}

#[test]
fn document_diagnostic_caching_changed() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content_v1 = "\
def foo() -> str:
    return 42
";
    let foo_content_v2 = "\
def foo() -> str:
    return \"fixed\"
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, ClientOptions::default())?
        .with_file(foo, foo_content_v1)?
        .enable_pull_diagnostics(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(foo, &foo_content_v1, 1);

    // First request with no previous result ID
    let first_response = server.document_diagnostic_request(foo, None)?;

    // Extract result ID from first response
    let result_id = match &first_response {
        lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(report),
        ) => report
            .full_document_diagnostic_report
            .result_id
            .as_ref()
            .expect("First response should have a result ID")
            .clone(),
        _ => panic!("First response should be a full report"),
    };

    // Change the document to fix the error
    server.change_text_document(
        foo,
        vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: foo_content_v2.to_string(),
        }],
        2,
    );

    // Second request with the previous result ID - should return a new full report
    let second_response = server.document_diagnostic_request(foo, Some(result_id))?;

    // Verify it's a full report (not unchanged)
    match second_response {
        lsp_types::DocumentDiagnosticReportResult::Report(
            lsp_types::DocumentDiagnosticReport::Full(report),
        ) => {
            // Should have no diagnostics now
            assert_eq!(report.full_document_diagnostic_report.items.len(), 0);
        }
        _ => panic!("Expected a full report when diagnostics have changed"),
    }

    Ok(())
}

#[test]
fn workspace_diagnostic_caching() -> Result<()> {
    let _filter = filter_result_id();

    let workspace_root = SystemPath::new("src");

    // File A: Will have an unchanged diagnostic
    let file_a = SystemPath::new("src/unchanged.py");
    let file_a_content = "\
def foo() -> str:
    return 42  # This error will remain the same
";

    // File B: Initially no error, will get a new error (added diagnostic)
    let file_b = SystemPath::new("src/new_error.py");
    let file_b_content_v1 = "\
def foo() -> int:
    return 42  # No error initially
";
    let file_b_content_v2 = "\
def foo() -> str:
    return 42  # Error appears
";

    // File C: Initially has error, will be fixed (removed diagnostic)
    let file_c = SystemPath::new("src/fixed_error.py");
    let file_c_content_v1 = "\
def foo() -> str:
    return 42  # Error initially
";
    let file_c_content_v2 = "\
def foo() -> str:
    return \"fixed\"  # Error removed
";

    // File D: Has error that changes content (changed diagnostic)
    let file_d = SystemPath::new("src/changed_error.py");
    let file_d_content_v1 = "\
def foo() -> str:
    return 42  # First error: expected str, got int
";
    let file_d_content_v2 = "\
def foo() -> int:
    return \"hello\"  # Different error: expected int, got str
";

    // File E: Modified but same diagnostic (e.g., new function added but original error remains)
    let file_e = SystemPath::new("src/modified_same_error.py");
    let file_e_content_v1 = "\
def foo() -> str:
    return 42  # Error: expected str, got int
";
    let file_e_content_v2 = "\
def bar() -> int:
    return 100  # New function added at the top

def foo() -> str:
    return 42  # Same error: expected str, got int
";

    let global_options = ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace);

    let mut server = TestServerBuilder::new()?
        .with_workspace(
            workspace_root,
            ClientOptions::default().with_diagnostic_mode(DiagnosticMode::Workspace),
        )?
        .with_initialization_options(global_options)
        .with_file(file_a, file_a_content)?
        .with_file(file_b, file_b_content_v1)?
        .with_file(file_c, file_c_content_v1)?
        .with_file(file_d, file_d_content_v1)?
        .with_file(file_e, file_e_content_v1)?
        .enable_pull_diagnostics(true)
        .build()?
        .wait_until_workspaces_are_initialized()?;

    server.open_text_document(file_a, &file_a_content, 1);

    // First request with no previous result IDs
    let first_response = server.workspace_diagnostic_request(None)?;

    insta::assert_debug_snapshot!("workspace_diagnostic_initial_state", first_response);

    // Consume all progress notifications sent during workspace diagnostics
    consume_all_progress_notifications(&mut server)?;

    // Extract result IDs from the first response
    let previous_result_ids = match first_response {
        WorkspaceDiagnosticReportResult::Report(report) => {
            report.items.into_iter().filter_map(|item| match item {
                WorkspaceDocumentDiagnosticReport::Full(full_report) => {
                    let result_id = full_report.full_document_diagnostic_report.result_id?;
                    Some(PreviousResultId {
                        uri: full_report.uri,
                        value: result_id,
                    })
                }
                WorkspaceDocumentDiagnosticReport::Unchanged(_) => {
                    panic!("The first response must be a full report, not unchanged");
                }
            })
        }
        WorkspaceDiagnosticReportResult::Partial(_) => {
            panic!("The first response must be a full report");
        }
    }
    .collect();

    // Make changes to files B, C, D, and E (leave A unchanged)
    // Need to open files before changing them
    server.open_text_document(file_b, &file_b_content_v1, 1);
    server.open_text_document(file_c, &file_c_content_v1, 1);
    server.open_text_document(file_d, &file_d_content_v1, 1);
    server.open_text_document(file_e, &file_e_content_v1, 1);

    // File B: Add a new error
    server.change_text_document(
        file_b,
        vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: file_b_content_v2.to_string(),
        }],
        2,
    );

    // File C: Fix the error
    server.change_text_document(
        file_c,
        vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: file_c_content_v2.to_string(),
        }],
        2,
    );

    // File D: Change the error
    server.change_text_document(
        file_d,
        vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: file_d_content_v2.to_string(),
        }],
        2,
    );

    // File E: Modify the file but keep the same diagnostic
    server.change_text_document(
        file_e,
        vec![lsp_types::TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: file_e_content_v2.to_string(),
        }],
        2,
    );

    // Second request with previous result IDs
    // Expected results:
    // - File A: Unchanged report (diagnostic hasn't changed)
    // - File B: Full report (new diagnostic appeared)
    // - File C: Full report with empty diagnostics (diagnostic was removed)
    // - File D: Full report (diagnostic content changed)
    // - File E: Full report (the range changes)
    let second_response = server.workspace_diagnostic_request(Some(previous_result_ids))?;

    // Consume all progress notifications sent during the second workspace diagnostics
    consume_all_progress_notifications(&mut server)?;

    insta::assert_debug_snapshot!("workspace_diagnostic_after_changes", second_response);

    Ok(())
}

// Redact result_id values since they are hash-based and non-deterministic
fn filter_result_id() -> insta::internals::SettingsBindDropGuard {
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r#""[a-f0-9]{16}""#, r#""[RESULT_ID]""#);
    settings.bind_to_scope()
}

fn consume_all_progress_notifications(server: &mut TestServer) -> Result<()> {
    // Always consume Begin
    let begin_params = server.await_notification::<lsp_types::notification::Progress>()?;

    // The params are already the ProgressParams type
    let lsp_types::ProgressParamsValue::WorkDone(lsp_types::WorkDoneProgress::Begin(_)) =
        begin_params.value
    else {
        return Err(anyhow::anyhow!("Expected Begin progress notification"));
    };

    // Consume Report notifications - there may be multiple based on number of files
    // Keep consuming until we hit the End notification
    loop {
        let params = server.await_notification::<lsp_types::notification::Progress>()?;

        if let lsp_types::ProgressParamsValue::WorkDone(lsp_types::WorkDoneProgress::End(_)) =
            params.value
        {
            // Found the End notification, we're done
            break;
        }
        // Otherwise it's a Report notification, continue
    }

    Ok(())
}
