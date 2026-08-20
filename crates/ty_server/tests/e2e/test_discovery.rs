use anyhow::Result;
use lsp_types::{LspRequestMethod, MessageDirection, PublishDiagnosticsNotification, Request};
use ruff_db::system::SystemPath;
use serde_json::json;
use ty_server::ClientOptions;

use crate::{TestServer, TestServerBuilder};

const CWD_FILTER: (&str, &str) = (
    r#""workingDirectory": ".+""#,
    r#""workingDirectory": "[CWD]""#,
);
const PROGRAM_FILTER: (&str, &str) = (r#""program": ".+""#, r#""program": "[PYTHON]""#);

enum DiscoverTests {}

impl Request for DiscoverTests {
    type Params = serde_json::Value;
    type Result = serde_json::Value;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/discoverTests");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

enum ResolveTestRunParams {}

impl Request for ResolveTestRunParams {
    type Params = serde_json::Value;
    type Result = serde_json::Value;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/resolveTestRunParams");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

fn build_server(
    workspace_root: &SystemPath,
    test_file: &SystemPath,
    test_content: &str,
) -> Result<TestServer> {
    let server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(test_file, test_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    Ok(server)
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

fn open_and_await_diagnostics(server: &mut TestServer, file: &SystemPath, content: &str) {
    server.open_text_document(file, content, 1);
    let _ = server.await_notification::<PublishDiagnosticsNotification>();
}

#[test]
fn discover_tests_in_document() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_example.py");
    let test_content = "\
def test_add():
    assert 1 + 1 == 2

class TestFoo:
    def test_bar(self):
        pass

def helper():
    pass
";

    let mut server = build_server(workspace_root, test_file, test_content)?;
    open_and_await_diagnostics(&mut server, test_file, test_content);

    let tests = server.send_request_await::<DiscoverTests>(json!({
        "textDocument": {
            "uri": server.file_uri(test_file),
        }
    }));

    insta::assert_json_snapshot!(tests, @r#"
    [
      {
        "id": "test_example.py::test_add",
        "kind": "function",
        "label": "test_add",
        "range": {
          "end": {
            "character": 12,
            "line": 0
          },
          "start": {
            "character": 4,
            "line": 0
          }
        }
      },
      {
        "id": "test_example.py::TestFoo",
        "kind": "class",
        "label": "TestFoo",
        "range": {
          "end": {
            "character": 13,
            "line": 3
          },
          "start": {
            "character": 6,
            "line": 3
          }
        }
      },
      {
        "id": "test_example.py::TestFoo::test_bar",
        "kind": "function",
        "label": "test_bar",
        "range": {
          "end": {
            "character": 16,
            "line": 4
          },
          "start": {
            "character": 8,
            "line": 4
          }
        }
      }
    ]
    "#);

    Ok(())
}

#[test]
fn resolve_test_run_params() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_example.py");
    let test_content = "\
def test_add():
    assert 1 + 1 == 2
";

    let mut server = build_server_with_python_env(workspace_root, test_file, test_content)?;
    open_and_await_diagnostics(&mut server, test_file, test_content);

    let params = server.send_request_await::<ResolveTestRunParams>(json!({
        "textDocument": {
            "uri": server.file_uri(test_file),
        },
        "testId": "test_example.py::test_add",
    }));

    insta::with_settings!({
        filters => vec![CWD_FILTER, PROGRAM_FILTER]
    }, {
        insta::assert_json_snapshot!(params, @r#"
        {
          "arguments": [
            "-m",
            "pytest",
            "test_example.py::test_add"
          ],
          "program": "[PYTHON]",
          "workingDirectory": "[CWD]"
        }
        "#);
    });

    Ok(())
}

#[test]
fn resolve_test_run_params_without_python_environment() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_example.py");
    let test_content = "\
def test_add():
    assert 1 + 1 == 2
";

    let mut server = build_server(workspace_root, test_file, test_content)?;
    open_and_await_diagnostics(&mut server, test_file, test_content);

    let params = server.send_request_await::<ResolveTestRunParams>(json!({
        "textDocument": {
            "uri": server.file_uri(test_file),
        },
        "testId": "test_example.py::test_add",
    }));

    insta::with_settings!({
        filters => vec![CWD_FILTER]
    }, {
        insta::assert_json_snapshot!(params, @r#"
        {
          "arguments": [
            "-m",
            "pytest",
            "test_example.py::test_add"
          ],
          "program": null,
          "workingDirectory": "[CWD]"
        }
        "#);
    });

    Ok(())
}

#[test]
fn resolve_test_run_params_for_unknown_test() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let test_file = SystemPath::new("src/test_example.py");
    let test_content = "\
def test_add():
    assert 1 + 1 == 2
";

    let mut server = build_server(workspace_root, test_file, test_content)?;
    open_and_await_diagnostics(&mut server, test_file, test_content);

    let params = server.send_request_await::<ResolveTestRunParams>(json!({
        "textDocument": {
            "uri": server.file_uri(test_file),
        },
        "testId": "test_example.py::test_gone",
    }));

    assert!(
        params.is_null(),
        "Expected a null response for an unknown test id, but got: {params}"
    );

    Ok(())
}
