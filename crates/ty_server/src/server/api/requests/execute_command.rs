use crate::capabilities::SupportedCommand;
use crate::server;
use crate::server::api::LSPResult;
use crate::server::api::RequestHandler;
use crate::server::api::traits::SyncRequestHandler;
use crate::session::Session;
use crate::session::client::Client;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use ruff_python_ast::name::Name;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::str::FromStr;
use ty_project::Db as _;

/// Serializable arguments for the `ty.runTest` command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RunTestArgs {
    cwd: String,
    program: String,
    args: Vec<String>,
    test_target: String,
}

impl RunTestArgs {
    pub(crate) fn new(
        cwd: &str,
        file_path: Option<&str>,
        class_names: &[Name],
        function_name: Option<&str>,
    ) -> Self {
        let mut test_target = file_path.unwrap_or_default().to_string();
        for class_name in class_names {
            if !test_target.is_empty() {
                test_target.push_str("::");
            }
            test_target.push_str(class_name);
        }
        if let Some(func) = function_name {
            if !test_target.is_empty() {
                test_target.push_str("::");
            }
            test_target.push_str(func);
        }
        // TODO: Decide what command to use. This is tricky because we don't have python executable.
        Self {
            cwd: cwd.to_string(),
            program: "uv".to_string(),
            args: vec!["run".to_string(), "pytest".to_string(), test_target.clone()],
            test_target,
        }
    }
}

pub(crate) struct ExecuteCommand;

impl RequestHandler for ExecuteCommand {
    type RequestType = req::ExecuteCommand;
}

impl SyncRequestHandler for ExecuteCommand {
    fn run(
        session: &mut Session,
        client: &Client,
        params: types::ExecuteCommandParams,
    ) -> server::Result<Option<serde_json::Value>> {
        let command = SupportedCommand::from_str(&params.command)
            .with_failure_code(ErrorCode::InvalidParams)?;

        match command {
            SupportedCommand::Debug => Ok(Some(serde_json::Value::String(
                debug_information(session).with_failure_code(ErrorCode::InternalError)?,
            ))),
            SupportedCommand::RunTest => {
                run_test(client, params.arguments).with_failure_code(ErrorCode::InvalidParams)
            }
        }
    }
}

/// Fallback test runner for editors that don't handle the `ty.runTest` command client-side.
/// Editors with native test UI should intercept this command and run tests themselves.
fn run_test(
    client: &Client,
    mut arguments: Vec<serde_json::Value>,
) -> crate::Result<Option<serde_json::Value>> {
    if arguments.len() != 1 {
        return Err(anyhow::anyhow!(
            "Wrong number of arguments for runTest want 1 found {}",
            arguments.len()
        ));
    }
    let run_test: RunTestArgs = serde_json::from_value(arguments.swap_remove(0))?;

    let client = client.clone();

    // TODO: This thread is not joined, we need to handle the cancel running tests on exit.
    std::thread::spawn(move || {
        let cwd = std::path::PathBuf::from(&run_test.cwd);
        let cmd_display = format!("{} {}", run_test.program, run_test.args.join(" "));

        match std::process::Command::new(&run_test.program)
            .args(&run_test.args)
            .current_dir(&cwd)
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    client.show_message(
                        format!("passed\n{stdout}\n command: {cmd_display}"),
                        types::MessageType::INFO,
                    );
                } else {
                    client.show_message(
                        format!("\nfailed\n{stdout}\n{stderr}\n command: {cmd_display}"),
                        types::MessageType::ERROR,
                    );
                }
            }
            Err(e) => {
                client.show_message(
                    format!("Failed to run `{cmd_display}`: {e}"),
                    types::MessageType::ERROR,
                );
            }
        }
    });

    Ok(Some(serde_json::Value::String(
        "Test execution started".to_string(),
    )))
}

/// Returns a string with detailed memory usage.
fn debug_information(session: &Session) -> crate::Result<String> {
    let mut buffer = String::new();

    writeln!(
        buffer,
        "Client capabilities: {:#?}",
        session.client_capabilities()
    )?;
    writeln!(
        buffer,
        "Position encoding: {:#?}",
        session.position_encoding()
    )?;
    writeln!(buffer, "Global settings: {:#?}", session.global_settings())?;
    writeln!(
        buffer,
        "Open text documents: {}",
        session.text_document_handles().count()
    )?;
    writeln!(buffer)?;

    for (root, workspace) in session.workspaces() {
        writeln!(buffer, "Workspace {root} ({})", workspace.url())?;
        writeln!(buffer, "Settings: {:#?}", workspace.settings())?;
        writeln!(buffer)?;
    }

    for db in session.project_dbs() {
        writeln!(buffer, "Project at {}", db.project().root(db))?;
        writeln!(buffer, "Settings: {:#?}", db.project().settings(db))?;
        writeln!(buffer)?;
        writeln!(
            buffer,
            "Memory report:\n{}",
            db.salsa_memory_dump().display_full()
        )?;
    }
    Ok(buffer)
}
