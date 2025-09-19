use crate::capabilities::SupportedCommand;
use crate::server;
use crate::server::api::LSPResult;
use crate::server::api::RequestHandler;
use crate::server::api::traits::SyncRequestHandler;
use crate::session::Session;
use crate::session::client::Client;
use lsp_server::ErrorCode;
use lsp_types::{self as types, request as req};
use std::fmt::Write;
use std::str::FromStr;

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

        if command == SupportedCommand::Debug {
            return Ok(Some(serde_json::Value::String(
                debug_information(session).with_failure_code(ErrorCode::InternalError)?,
            )));
        }
        Ok(None)
    }
}

/// Returns a string with detailed memory usage.
fn debug_information(session: &Session) -> crate::Result<String> {
    let mut buffer = String::new();

    let db = session.project_dbs().next();
    match db {
        Some(db) => {
            //salsa has different kind of mem reports, for debug we want the full one.
            let db_str = db.salsa_memory_dump().display_full().to_string();
            writeln!(buffer, "{db_str}")?;
        }
        None => writeln!(buffer, "No db found")?,
    }
    Ok(buffer)
}
