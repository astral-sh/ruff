use anyhow::Result;
use lsp_types::{ExecuteCommandParams, WorkDoneProgressParams, request::ExecuteCommand};
use ruff_db::system::SystemPath;

use crate::{TestServer, TestServerBuilder};

// Sends an executeCommand request to the TestServer
fn execute_command(
    server: &mut TestServer,
    command: String,
    arguments: Vec<serde_json::Value>,
) -> Option<serde_json::Value> {
    let params = ExecuteCommandParams {
        command,
        arguments,
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let id = server.send_request::<ExecuteCommand>(params);
    server.await_response::<ExecuteCommand>(&id)
}

#[test]
fn debug_command() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let foo_content = "\
def foo() -> str:
return 42
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let response = execute_command(&mut server, "ty.printDebugInformation".to_string(), vec![]);
    let response = response.expect("expect server response");

    let response = response
        .as_str()
        .expect("debug command to return a string response");

    insta::with_settings!({
        filters => vec![
            (r"\b[0-9]+.[0-9]+MB\b", "[X.XXMB]"),
            (r"Workspace .+\)", "Workspace XXX"),
            (r"Project at .+", "Project at XXX"),
            (r"rules: \{(.|\n)+?\}\,", "rules: <RULES>,"),
    ]}, {
        insta::assert_snapshot!(response);
    });

    Ok(())
}
