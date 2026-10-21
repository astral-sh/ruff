use lsp_types::{LspRequestMethod, MessageDirection, Request};
use ruff_db::system::SystemPath;
use serde::{Deserialize, Serialize};
use ty_project::Db as _;

use super::discover_tests::path_is_in_project;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;

/// `ty/resolveTestRunParams` resolves how to run a test that was previously discovered through `ty/discoverTests`.
pub(crate) enum ResolveTestRunParamsRequest {}

impl Request for ResolveTestRunParamsRequest {
    type Params = ResolveTestRunParamsParams;
    type Result = Option<TestRunParams>;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/resolveTestRunParams");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolveTestRunParamsParams {
    /// The id of the test, file, or directory to resolve, as returned by `ty/discoverTests`.
    test_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestRunParams {
    /// The directory the test should be run from (the project root).
    working_directory: String,
    /// The arguments to pass to a Python interpreter to run the test with pytest.
    arguments: Vec<String>,
}

pub(crate) struct ResolveTestRunParamsRequestHandler;

impl RequestHandler for ResolveTestRunParamsRequestHandler {
    type RequestType = ResolveTestRunParamsRequest;
}

impl BackgroundRequestHandler for ResolveTestRunParamsRequestHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: ResolveTestRunParamsParams,
    ) -> crate::server::Result<Option<TestRunParams>> {
        tracing::debug!("Resolving test run params for `{}`", params.test_id);

        let path = params
            .test_id
            .split_once("::")
            .map_or(params.test_id.as_str(), |(path, _)| path);

        let path = SystemPath::new(path);
        let Some((workspace_root, db)) = snapshot.project_db_for_path(path) else {
            tracing::debug!("`{path}` is not inside any workspace folder; returning null");
            return Ok(None);
        };

        if !path_is_in_project(db, workspace_root, path) {
            tracing::debug!("`{path}` is not discoverable in `{workspace_root}`; returning null");
            return Ok(None);
        }

        let working_directory = db.project().root(db).to_string();

        Ok(Some(TestRunParams {
            working_directory,
            arguments: vec![
                "-m".to_string(),
                "pytest".to_string(),
                "-vv".to_string(),
                params.test_id,
            ],
        }))
    }
}

impl RetriableRequestHandler for ResolveTestRunParamsRequestHandler {}
