use crate::capabilities::SupportedCommand;
use crate::server;
use crate::server::api::LSPResult;
use crate::server::api::RequestHandler;
use crate::server::api::traits::SyncRequestHandler;
use crate::session::Session;
use crate::session::client::Client;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use ruff_db::system::SystemPath;
use serde::{Deserialize, Serialize};
use std::fmt::Write;
use std::str::FromStr;
use ty_project::Db as _;
use ty_python_semantic::Program;

/// Arguments for the `ty.runTest` command.
///
/// The `program` and `arguments` fields are provided as a convenience for clients
/// that want to run the test command directly. The server does not use these fields
/// when executing tests, it reconstructs the command.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunTestArgs {
    cwd: String,
    program: String,
    // Full arguments to call program with and execute the tests.
    arguments: Vec<String>,
    // Path of the file that contains the test.
    file_path: String,
    // qualified test name e.g. `Class::test_func`
    test_target: String,
}

impl RunTestArgs {
    pub(crate) fn new(
        cwd: &str,
        file_path: String,
        test_target: String,
        python_executable: &SystemPath,
    ) -> Self {
        let arguments = vec![
            "-m".to_string(),
            "pytest".to_string(),
            format!("{file_path}::{test_target}"),
        ];

        Self {
            cwd: cwd.to_string(),
            program: python_executable.to_string(),
            arguments,
            file_path,
            test_target,
        }
    }
}

impl std::fmt::Display for RunTestArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} {} in {}",
            self.program,
            self.arguments.join(" "),
            self.cwd
        )
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
            SupportedCommand::RunTest => run_test(session, client, params.arguments)
                .with_failure_code(ErrorCode::InvalidParams),
        }
    }
}

/// Fallback test runner for editors that don't handle the `ty.runTest` command client-side.
/// Editors with native test UI should intercept this command and run tests themselves.
///
// TODO: Consider adding an option argument to control whether output is returned in the
// response or shown via `show_message`.
fn run_test(
    session: &Session,
    client: &Client,
    mut arguments: Vec<serde_json::Value>,
) -> crate::Result<Option<serde_json::Value>> {
    if arguments.len() != 1 {
        return Err(anyhow::anyhow!(
            "Wrong number of arguments for runTest want 1 found {}",
            arguments.len()
        ));
    }
    let run_test_args: RunTestArgs = serde_json::from_value(arguments.swap_remove(0))?;
    let db = session
        .project_db_for_path(&run_test_args.cwd)
        .ok_or_else(|| {
            anyhow::anyhow!("No project database found for path: {}", run_test_args.cwd)
        })?;
    let python_executable = Program::get(db)
        .python_executable(db)
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("No Python executable found."))?;

    // We reconstruct the command using the server's known python executable so the client
    // cannot choose an arbitrary program to execute. However the risk still remains as we take
    // input from the client.
    let run_test_args = RunTestArgs::new(
        &run_test_args.cwd,
        run_test_args.file_path,
        run_test_args.test_target,
        python_executable,
    );
    let client = client.clone();

    // TODO: This thread is not joined, we need to cancel running tests on exit.
    std::thread::spawn(move || {
        match std::process::Command::new(&run_test_args.program)
            .args(&run_test_args.arguments)
            .current_dir(std::path::PathBuf::from(&run_test_args.cwd))
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    client.show_message(
                        format!("passed\n{stdout}\n command: {run_test_args}"),
                        types::MessageType::INFO,
                    );
                } else {
                    client.show_message(
                        format!("\nfailed\n{stdout}\n{stderr}\n command: {run_test_args}"),
                        types::MessageType::ERROR,
                    );
                }
            }
            Err(e) => {
                client.show_message(
                    format!("Failed to run `{run_test_args}`: {e}"),
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
