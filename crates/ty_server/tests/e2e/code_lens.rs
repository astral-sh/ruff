use anyhow::Result;
use lsp_types::{
    CodeLensParams, CodeLensRequest, PartialResultParams, PublishDiagnosticsNotification,
    TextDocumentIdentifier, WorkDoneProgressParams,
};
use ruff_db::system::SystemPath;

use crate::{TestServer, TestServerBuilder};

const URI_FILTER: (&str, &str) = (r#""uri": ".+""#, r#""uri": "[URI]""#);

fn code_lens_request(server: &mut TestServer, file: &SystemPath) -> Vec<lsp_types::CodeLens> {
    let params = CodeLensParams {
        text_document: TextDocumentIdentifier {
            uri: server.file_uri(file),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let id = server.send_request::<CodeLensRequest>(params);
    server
        .await_response::<CodeLensRequest>(&id)
        .unwrap_or_default()
}

fn build_server(
    workspace_root: &SystemPath,
    test_file: &SystemPath,
    test_content: &str,
) -> Result<TestServer> {
    let server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_file, test_content)?
        .with_run_tests_support()
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    Ok(server)
}

#[test]
fn code_lens_for_test_functions() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_example.py");
    let test_content = "\
def test_add():
    assert 1 + 1 == 2

def helper():
    pass
";

    let mut server = build_server(workspace_root, test_file, test_content)?;

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnosticsNotification>();

    let lenses = code_lens_request(&mut server, test_file);

    insta::with_settings!({
        filters => vec![URI_FILTER]
    }, {
        insta::assert_json_snapshot!(lenses, @r#"
        [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 4
              },
              "end": {
                "line": 0,
                "character": 12
              }
            },
            "command": {
              "title": "Run test",
              "command": "ty.runTest",
              "arguments": [
                {
                  "kind": "function",
                  "label": "test_add",
                  "testId": "test_example.py::test_add",
                  "uri": "[URI]"
                }
              ]
            }
          }
        ]
        "#);
    });

    Ok(())
}

#[test]
fn code_lens_for_test_classes() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_classes.py");
    let test_content = "\
class TestFoo:
    def test_bar(self):
        pass

    def helper(self):
        pass
";

    let mut server = build_server(workspace_root, test_file, test_content)?;

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnosticsNotification>();

    let lenses = code_lens_request(&mut server, test_file);

    insta::with_settings!({
        filters => vec![URI_FILTER]
    }, {
        insta::assert_json_snapshot!(lenses, @r#"
        [
          {
            "range": {
              "start": {
                "line": 0,
                "character": 6
              },
              "end": {
                "line": 0,
                "character": 13
              }
            },
            "command": {
              "title": "Run tests",
              "command": "ty.runTest",
              "arguments": [
                {
                  "kind": "class",
                  "label": "TestFoo",
                  "testId": "test_classes.py::TestFoo",
                  "uri": "[URI]"
                }
              ]
            }
          },
          {
            "range": {
              "start": {
                "line": 1,
                "character": 8
              },
              "end": {
                "line": 1,
                "character": 16
              }
            },
            "command": {
              "title": "Run test",
              "command": "ty.runTest",
              "arguments": [
                {
                  "kind": "function",
                  "label": "test_bar",
                  "testId": "test_classes.py::TestFoo::test_bar",
                  "uri": "[URI]"
                }
              ]
            }
          }
        ]
        "#);
    });

    Ok(())
}

#[test]
fn code_lens_skipped_without_run_tests_support() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_example.py");
    let test_content = "\
def test_add():
    assert 1 + 1 == 2
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_file, test_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnosticsNotification>();

    let lenses = code_lens_request(&mut server, test_file);

    assert!(
        lenses.is_empty(),
        "Expected no code lenses for a client that cannot run tests, but got {lenses:?}"
    );

    Ok(())
}
