use crate::capabilities::SupportedCommand;
use crate::server;
use crate::server::api::LSPResult;
use crate::server::api::RequestHandler;
use crate::server::api::traits::SyncRequestHandler;
use crate::session::Session;
use crate::session::client::Client;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use std::fmt::{self, Write};
use std::str::FromStr;
use ty_module_resolver::ModuleResolveMode;
use ty_project::Db as _;
use ty_python_core::program::Program;

pub(crate) struct ExecuteCommand;

impl RequestHandler for ExecuteCommand {
    type RequestType = req::ExecuteCommand;
}

impl SyncRequestHandler for ExecuteCommand {
    fn run(
        session: &mut Session,
        _client: &Client,
        params: types::ExecuteCommandParams,
    ) -> server::Result<Option<serde_json::Value>> {
        let command = SupportedCommand::from_str(&params.command)
            .with_failure_code(ErrorCode::InvalidParams)?;

        match command {
            SupportedCommand::Debug => Ok(Some(serde_json::Value::String(
                debug_information(session).with_failure_code(ErrorCode::InternalError)?,
            ))),
        }
    }
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
        let program = Program::get(db);
        writeln!(buffer, "Program:")?;
        writeln!(
            buffer,
            "  python-version: {}",
            program.python_version_with_source(db).version
        )?;
        writeln!(buffer, "  python-platform: {}", program.python_platform(db))?;
        let mut writer = IndentingWriter {
            inner: &mut buffer,
            indent: "  ",
            at_line_start: false,
        };
        writeln!(
            writer,
            "  search-paths: {:#}",
            program
                .search_paths(db)
                .display(db, ModuleResolveMode::StubsAllowed)
        )?;

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

struct IndentingWriter<'a> {
    inner: &'a mut String,
    indent: &'static str,
    at_line_start: bool,
}

impl Write for IndentingWriter<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for part in s.split_inclusive('\n') {
            if self.at_line_start {
                self.inner.write_str(self.indent)?;
            }
            self.inner.write_str(part)?;
            self.at_line_start = part.ends_with('\n');
        }

        Ok(())
    }
}
