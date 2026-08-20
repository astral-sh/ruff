use anyhow::Result;
use lsp_types::{
    CodeLensParams, PartialResultParams, TextDocumentIdentifier, WorkDoneProgressParams,
    notification::PublishDiagnostics,
};
use ruff_db::system::SystemPath;
use serde_json::json;
use ty_server::ClientOptions;

use crate::{TestServer, TestServerBuilder};

const CWD_FILTER: (&str, &str) = (r#""cwd": ".+""#, r#""cwd": "[CWD]""#);
const PROGRAM_FILTER: (&str, &str) = (r#""program": ".+""#, r#""program": "[PYTHON]""#);

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

fn build_server_with_python_env(
    workspace_root: &SystemPath,
    test_file: &SystemPath,
    test_content: &str,
) -> Result<TestServer> {
    let builder = TestServerBuilder::new()?;

    let python_home = builder.file_path("base/bin");
    let sys_prefix = builder.file_path(".venv");
    let base_python = if cfg!(target_os = "windows") {
        "base/bin/python.exe"
    } else {
        "base/bin/python"
    };
    let venv_python = if cfg!(target_os = "windows") {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    };
    let python_uri = builder.file_uri(venv_python);
    let site_packages = if cfg!(target_os = "windows") {
        ".venv/Lib/site-packages/.gitkeep"
    } else {
        ".venv/lib/python3.14/site-packages/.gitkeep"
    };

    let workspace_options: ClientOptions = serde_json::from_value(json!({
        "pythonExtension": {
            "activeEnvironment": {
                "executable": {
                    "uri": python_uri,
                    "sysPrefix": sys_prefix,
                }
            }
        }
    }))?;

    let server = builder
        .with_workspace(workspace_root, Some(workspace_options))?
        .with_file(SystemPath::new(base_python), "")?
        .with_file(SystemPath::new(venv_python), "")?
        .with_file(
            SystemPath::new(".venv/pyvenv.cfg"),
            format!("home = {python_home}\n"),
        )?
        .with_file(SystemPath::new(site_packages), "")?
        .with_file(test_file, test_content)?
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

    let mut server = build_server_with_python_env(workspace_root, test_file, test_content)?;

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let lenses = code_lens_request(&mut server, test_file);

    insta::with_settings!({
        filters => vec![CWD_FILTER, PROGRAM_FILTER]
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
                    "-m",
                    "pytest",
                    "test_example.py::test_add"
                  ],
                  "cwd": "[CWD]",
                  "filePath": "test_example.py",
                  "program": "[PYTHON]",
                  "testTarget": "test_add"
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

    let mut server = build_server_with_python_env(workspace_root, test_file, test_content)?;

    server.open_text_document(test_file, test_content, 1);
    let _ = server.await_notification::<PublishDiagnostics>();

    let lenses = code_lens_request(&mut server, test_file);

    insta::with_settings!({
        filters => vec![CWD_FILTER, PROGRAM_FILTER]
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
                    "-m",
                    "pytest",
                    "test_classes.py::TestFoo"
                  ],
                  "cwd": "[CWD]",
                  "filePath": "test_classes.py",
                  "program": "[PYTHON]",
                  "testTarget": "TestFoo"
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
                    "-m",
                    "pytest",
                    "test_classes.py::TestFoo::test_bar"
                  ],
                  "cwd": "[CWD]",
                  "filePath": "test_classes.py",
                  "program": "[PYTHON]",
                  "testTarget": "TestFoo::test_bar"
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
fn code_lens_skipped_without_python_environment() -> Result<()> {
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
    let _ = server.await_notification::<PublishDiagnostics>();

    let lenses = code_lens_request(&mut server, test_file);

    assert!(
        lenses.is_empty(),
        "Expected no code lenses without a Python environment, but got {lenses:?}"
    );

    Ok(())
}
