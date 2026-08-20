use anyhow::Result;
use lsp_types::{
    CodeLensParams, PartialResultParams, TextDocumentIdentifier, WorkDoneProgressParams,
    notification::PublishDiagnostics,
};
use ruff_db::system::SystemPath;

use crate::{TestServer, TestServerBuilder};

const CWD_FILTER: (&str, &str) = (r#""cwd": ".+""#, r#""cwd": "[CWD]""#);

fn code_lens_request(server: &mut TestServer, file: &SystemPath) -> Vec<lsp_types::CodeLens> {
    let params = CodeLensParams {
        text_document: TextDocumentIdentifier {
            uri: server.file_uri(file),
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    let id = server.send_request::<lsp_types::request::CodeLensRequest>(params);
    server
        .await_response::<lsp_types::request::CodeLensRequest>(&id)
        .unwrap_or_default()
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

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_file, test_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let lenses = code_lens_request(&mut server, test_file);

    insta::with_settings!({
        filters => vec![CWD_FILTER]
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
                  "arguments": [
                    "run",
                    "pytest",
                    "test_example.py::test_add"
                  ],
                  "cwd": "[CWD]",
                  "program": "uv",
                  "testTarget": "test_example.py::test_add"
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

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_file, test_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let lenses = code_lens_request(&mut server, test_file);

    insta::with_settings!({
        filters => vec![CWD_FILTER]
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
                  "arguments": [
                    "run",
                    "pytest",
                    "test_classes.py::TestFoo"
                  ],
                  "cwd": "[CWD]",
                  "program": "uv",
                  "testTarget": "test_classes.py::TestFoo"
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
                  "arguments": [
                    "run",
                    "pytest",
                    "test_classes.py::TestFoo::test_bar"
                  ],
                  "cwd": "[CWD]",
                  "program": "uv",
                  "testTarget": "test_classes.py::TestFoo::test_bar"
                }
              ]
            }
          }
        ]
        "#);
    });

    Ok(())
}
