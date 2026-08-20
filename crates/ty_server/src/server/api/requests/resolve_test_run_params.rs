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

/// Custom `ty/resolveTestRunParams` request that resolves how to run a test that was
/// previously discovered through `ty/discoverTests`.
///
/// The server never runs tests itself; it only describes the command so the client can
/// execute it with its own process management, cancellation, and output handling.
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
    pub(crate) test_id: String,
}

/// Describes how to run a test with pytest.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestRunParams {
    /// The directory the test should be run from (the project root).
    pub(crate) working_directory: String,
    /// The Python interpreter that ty discovered for the project, if any.
    pub(crate) program: Option<String>,
    /// The arguments to pass to a Python interpreter to run the test with pytest.
    pub(crate) arguments: Vec<String>,
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

        let program = db
            .project()
            .program(db)
            .python_executable(db)
            .as_deref()
            .map(ToString::to_string);

        let working_directory = db.project().root(db).to_string();

        tracing::debug!(
            "Resolved `{}` to run from `{working_directory}` with interpreter {program:?}",
            params.test_id
        );

        Ok(Some(TestRunParams {
            working_directory,
            program,
            arguments: vec!["-m".to_string(), "pytest".to_string(), params.test_id],
        }))
    }
}

impl RetriableRequestHandler for ResolveTestRunParamsRequestHandler {}
