//! Handler for `typeServer/getPythonSearchPaths`.

use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::tsp::handlers::snapshot::validate_snapshot;
use crate::tsp::requests::{GetPythonSearchPaths, GetPythonSearchPathsParams};
use anyhow::anyhow;
use lsp_server::ErrorCode;
use ty_module_resolver::ModuleResolveMode;
use ty_project::Db;

pub(crate) struct GetPythonSearchPathsHandler;

impl RequestHandler for GetPythonSearchPathsHandler {
    type RequestType = GetPythonSearchPaths;
}

impl RetriableRequestHandler for GetPythonSearchPathsHandler {}

impl BackgroundRequestHandler for GetPythonSearchPathsHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: GetPythonSearchPathsParams,
    ) -> crate::server::Result<Option<Vec<String>>> {
        if let Err(e) = validate_snapshot(params.snapshot, snapshot.revision()) {
            return Err(crate::server::api::Error::new(
                anyhow!(e.message),
                ErrorCode::ServerCancelled,
            ));
        }

        // Find the project database that best matches fromUri.
        // Fall back to the first project if no match is found.
        let from_path = lsp_types::Url::parse(&params.from_uri)
            .ok()
            .and_then(|url| url.to_file_path().ok());

        let db = if let Some(ref path) = from_path {
            snapshot
                .projects()
                .iter()
                .find(|db| {
                    let root = db.project().root(*db).as_std_path();
                    path.starts_with(root)
                })
                .or_else(|| snapshot.projects().first())
        } else {
            snapshot.projects().first()
        };

        let Some(db) = db else {
            return Ok(None);
        };

        let paths: Vec<String> =
            ty_module_resolver::search_paths(db, ModuleResolveMode::StubsAllowed)
                .filter_map(|sp| sp.as_system_path().map(ToString::to_string))
                .collect();

        Ok(Some(paths))
    }
}
