use anyhow::Result;
use lsp_types::{LspRequestMethod, MessageDirection, Request};
use ruff_db::system::SystemPath;
use serde_json::{Value, json};

use crate::{TestServer, TestServerBuilder};

enum ResolveTestRunParams {}

impl Request for ResolveTestRunParams {
    type Params = Value;
    type Result = Value;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/resolveTestRunParams");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

const MODULE: &str = "src/tests/test_module.py";
const MODULE_CONTENT: &str = "\
def test_one():
    pass


class TestThings:
    def test_method(self):
        pass
";

fn server_with_one_test_module() -> Result<TestServer> {
    let server = TestServerBuilder::new()?
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(SystemPath::new(MODULE), MODULE_CONTENT)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    Ok(server)
}

/// Returns the id that `ty/discoverTests` reports for `path`, optionally qualified by the
/// name of a test inside it.
fn test_id(server: &TestServer, path: &str, qualified_name: Option<&str>) -> String {
    let path = server.file_path(SystemPath::new(path));
    match qualified_name {
        Some(name) => format!("{path}::{name}"),
        None => path.to_string(),
    }
}

#[test]
fn resolve_run_params_for_a_file() -> Result<()> {
    let mut server = server_with_one_test_module()?;

    let id = test_id(&server, MODULE, None);
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/src/tests/test_module.py"
      ],
      "program": null,
      "workingDirectory": "<temp_dir>/src"
    }
    "#);

    Ok(())
}

#[test]
fn resolve_run_params_for_a_directory() -> Result<()> {
    let mut server = server_with_one_test_module()?;

    let id = test_id(&server, "src/tests", None);
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/src/tests"
      ],
      "program": null,
      "workingDirectory": "<temp_dir>/src"
    }
    "#);

    Ok(())
}

#[test]
fn resolve_run_params_for_a_function() -> Result<()> {
    let mut server = server_with_one_test_module()?;

    let id = test_id(&server, MODULE, Some("test_one"));
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/src/tests/test_module.py::test_one"
      ],
      "program": null,
      "workingDirectory": "<temp_dir>/src"
    }
    "#);

    Ok(())
}

#[test]
fn resolve_run_params_for_a_class() -> Result<()> {
    let mut server = server_with_one_test_module()?;

    let id = test_id(&server, MODULE, Some("TestThings"));
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/src/tests/test_module.py::TestThings"
      ],
      "program": null,
      "workingDirectory": "<temp_dir>/src"
    }
    "#);

    Ok(())
}

#[cfg(unix)]
#[test]
fn resolve_run_params_with_a_project_virtual_environment() -> Result<()> {
    let builder = TestServerBuilder::new()?;
    let python_home = builder.file_path(SystemPath::new("base/bin"));

    let mut server = builder
        .with_workspace(SystemPath::new("src"), None)?
        .with_file(SystemPath::new("base/bin/python"), "")?
        .with_file(
            SystemPath::new("src/.venv/pyvenv.cfg"),
            format!("home = {python_home}\n"),
        )?
        .with_file(SystemPath::new("src/.venv/bin/python"), "")?
        .with_file(
            SystemPath::new("src/.venv/lib/python3.13/site-packages/.gitkeep"),
            "",
        )?
        .with_file(SystemPath::new(MODULE), MODULE_CONTENT)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let id = test_id(&server, MODULE, Some("test_one"));
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/src/tests/test_module.py::test_one"
      ],
      "program": "<temp_dir>/src/.venv/bin/python",
      "workingDirectory": "<temp_dir>/src"
    }
    "#);

    Ok(())
}

/// A client can hold on to an id for a module that has since been deleted or renamed. The
/// server answers `null` so the client knows to discover the tests again.
#[test]
fn resolve_run_params_for_a_path_that_is_not_in_the_project() -> Result<()> {
    let mut server = server_with_one_test_module()?;

    let id = test_id(&server, "src/tests/test_deleted.py", None);
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @"null");

    Ok(())
}

#[test]
fn resolve_run_params_for_a_module_with_same_relative_path_in_another_workspace() -> Result<()> {
    let workspace_one = SystemPath::new("workspace_one");
    let workspace_two = SystemPath::new("workspace_two");

    // Both workspaces have a `tests/test_module.py` at the same relative path, so each id
    // has to resolve against the workspace it actually belongs to.
    let module_one = SystemPath::new("workspace_one/tests/test_module.py");
    let module_one_content = "\
def test_alpha():
    pass
";
    let module_two = SystemPath::new("workspace_two/tests/test_module.py");
    let module_two_content = "\
def test_beta():
    pass
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_one, None)?
        .with_file(module_one, module_one_content)?
        .with_workspace(workspace_two, None)?
        .with_file(module_two, module_two_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let id = test_id(
        &server,
        "workspace_one/tests/test_module.py",
        Some("test_alpha"),
    );
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/workspace_one/tests/test_module.py::test_alpha"
      ],
      "program": null,
      "workingDirectory": "<temp_dir>/workspace_one"
    }
    "#);

    let id = test_id(
        &server,
        "workspace_two/tests/test_module.py",
        Some("test_beta"),
    );
    let params = server.send_request_await::<ResolveTestRunParams>(json!({ "testId": id }));

    insta::assert_json_snapshot!(params, @r#"
    {
      "arguments": [
        "-m",
        "pytest",
        "<temp_dir>/workspace_two/tests/test_module.py::test_beta"
      ],
      "program": null,
      "workingDirectory": "<temp_dir>/workspace_two"
    }
    "#);

    Ok(())
}
