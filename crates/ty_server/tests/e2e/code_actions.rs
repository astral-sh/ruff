use crate::{TestServer, TestServerBuilder};
use anyhow::Result;
use lsp_types::{DocumentDiagnosticReportResult, Position, Range, request::CodeActionRequest};
use ruff_db::system::SystemPath;

fn code_actions_at(
    server: &TestServer,
    diagnostics: DocumentDiagnosticReportResult,
    file: &SystemPath,
    range: Range,
) -> lsp_types::CodeActionParams {
    lsp_types::CodeActionParams {
        text_document: lsp_types::TextDocumentIdentifier {
            uri: server.file_uri(file),
        },
        range,
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
    }
}

#[allow(clippy::cast_possible_truncation)]
fn full_range(input: &str) -> Range {
    let (num_lines, last_line) = input
        .lines()
        .enumerate()
        .last()
        .expect("non-empty document");
    let last_char = last_line.len() as u32;
    Range::new(
        Position::new(0, 0),
        Position::new(num_lines as u32, last_char),
    )
}

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

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions for the line with the unused ignore comment.
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

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);

    // Get code actions for a range that doesn't overlap with the diagnostic.
    // The diagnostic is at characters 12-42, so we request actions for characters 0-10.
    let range = Range::new(Position::new(0, 0), Position::new(0, 10));
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    // Should return None because the range doesn't overlap with the diagnostic.
    assert_eq!(code_actions, None);

    Ok(())
}

// `Literal` is available from two places so we should suggest two possible imports
#[test]
fn code_action_undefined_reference_multi() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: Literal[1] = 1
";

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions
    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}

// Using an unimported decorator `@deprecated`
#[test]
fn code_action_undefined_decorator() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"\
@deprecated("do not use!!!")
def my_func(): ...
"#;

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions
    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}

// Using an unimported decorator `@deprecated`
#[test]
fn code_action_existing_import_undefined_decorator() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"\
import warnings

@deprecated("do not use!!!")
def my_func(): ...
"#;

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions
    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}

// Accessing `typing.Literal` without `typing` imported (ideally we suggest importing `typing`)
#[test]
fn code_action_attribute_access_on_unimported() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
x: typing.Literal[1] = 1
";

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions
    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}

// Accessing `html.parser` when we've imported `html` but not `html.parser`
#[test]
fn code_action_possible_missing_submodule_attribute() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
import html
html.parser
";

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions
    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}

/// Regression test for a panic when a code-fix diagnostic points at a string annotation
#[test]
fn code_action_invalid_string_annotations() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = r#"
ab: "foobar"
"#;

    let ty_toml = SystemPath::new("ty.toml");
    let ty_toml_content = "";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(ty_toml, ty_toml_content)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(true)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(foo, foo_content, 1);

    // Wait for diagnostics to be computed.
    let diagnostics = server.document_diagnostic_request(foo, None);
    let range = full_range(foo_content);
    let code_action_params = code_actions_at(&server, diagnostics, foo, range);

    // Get code actions for the line with the unused ignore comment.
    let code_action_id = server.send_request::<CodeActionRequest>(code_action_params);
    let code_actions = server.await_response::<CodeActionRequest>(&code_action_id);

    insta::assert_json_snapshot!(code_actions);

    Ok(())
}
