use lsp_types::{LspRequestMethod, MessageDirection, Request};
use ruff_db::system::SystemPath;
use serde::{Deserialize, Serialize};
use ty_project::Db as _;

use super::discover_tests::project_includes_path;
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

        let Some(db) = snapshot
            .projects()
            .iter()
            .find(|db| project_includes_path(db, SystemPath::new(path)))
        else {
            tracing::debug!("No open project includes `{path}`; returning null");
            return Ok(None);
        };

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
