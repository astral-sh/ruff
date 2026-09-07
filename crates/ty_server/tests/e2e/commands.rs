use anyhow::{Context, Result};
use lsp_types::{ExecuteCommandParams, ExecuteCommandRequest, WorkDoneProgressParams};
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
        arguments: Some(arguments),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    let id = server.send_request::<ExecuteCommandRequest>(params);
    server.await_response::<ExecuteCommandRequest>(&id)
}

#[test]
fn debug_command() -> Result<()> {
    let workspace_root = SystemPath::new("src");
    let foo = SystemPath::new("src/foo.py");
    let ty_toml = SystemPath::new("ty.toml");
    let foo_content = "\
def foo() -> str:
return 42
";
    let ty_toml_content = "\
[environment]
python-version = \"3.10\"
python-platform = \"linux\"
";

    let mut server = TestServerBuilder::new()?
        .with_workspace(workspace_root, None)?
        .with_file(foo, foo_content)?
        .with_file(ty_toml, ty_toml_content)?
        .enable_pull_diagnostics(false)
        .build()
        .wait_until_workspaces_are_initialized();

    let response = execute_command(&mut server, "ty.printDebugInformation".to_string(), vec![]);
    let response = response.expect("expect server response");

    let response = response
        .as_str()
        .expect("debug command to return a string response");

    let (before_structs, salsa_structs) = response
        .split_once("=======SALSA STRUCTS=======\n")
        .context("debug response missing Salsa structs section")?;
    let (salsa_structs, salsa_queries) = salsa_structs
        .split_once("=======SALSA QUERIES=======\n")
        .context("debug response missing Salsa queries section")?;
    let (salsa_queries, summary) = salsa_queries
        .split_once("=======SALSA SUMMARY=======\n")
        .context("debug response missing Salsa summary section")?;

    // Memory usage varies between platforms and build profiles. Sort entries by name instead.
    let mut salsa_structs = salsa_structs.lines().collect::<Vec<_>>();
    salsa_structs.sort_unstable();
    let query_lines = salsa_queries.lines().collect::<Vec<_>>();
    let mut salsa_queries = query_lines
        .chunks(2)
        .map(|query| query.join("\n"))
        .collect::<Vec<_>>();
    salsa_queries.sort_unstable();
    let response = format!(
        "{before_structs}=======SALSA STRUCTS=======\n{}\n=======SALSA QUERIES=======\n{}\n=======SALSA SUMMARY=======\n{summary}",
        salsa_structs.join("\n"),
        salsa_queries.join("\n")
    );

    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\b[0-9]+.[0-9]+MB\b", "[X.XXMB]");
    settings.add_filter(r"Workspace .+\)", "Workspace XXX");
    settings.add_filter(r"Project at .+", "Project at XXX");
    settings.add_filter(r"(?m)^(\s+).*/site-packages,$", "$1<site-packages>,");
    settings.add_filter(r"rules: \{(.|\n)+?\}\,", "rules: <RULES>,");
    let _settings = settings.bind_to_scope();

    insta::assert_snapshot!(response);

    Ok(())
}
