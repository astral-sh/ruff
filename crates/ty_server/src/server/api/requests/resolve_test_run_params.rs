use std::borrow::Cow;

use lsp_types::{LspRequestMethod, MessageDirection, Request, TextDocumentIdentifier, Uri};
use serde::{Deserialize, Serialize};
use ty_ide::discover_tests;
use ty_project::{Db as _, ProjectDatabase};
use ty_python_core::program::Program;

use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

/// Custom `ty/resolveTestRunParams` request that resolves how to run a test that was
/// previously discovered through `ty/discoverTests` or a test code lens.
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
    /// The document the test was discovered in.
    pub(crate) text_document: TextDocumentIdentifier,
    /// The id of the test, as returned by `ty/discoverTests` or attached to a test code
    /// lens, e.g. `tests/test_foo.py::TestFoo::test_bar`.
    pub(crate) test_id: String,
}

/// Describes how to run a test with pytest.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TestRunParams {
    /// The directory the test should be run from (the project root).
    pub(crate) working_directory: String,
    /// The Python interpreter that ty discovered for the project, if any.
    ///
    /// Clients that do their own interpreter discovery can ignore this and only use
    /// `arguments`.
    pub(crate) program: Option<String>,
    /// The arguments to pass to a Python interpreter to run the test with pytest.
    pub(crate) arguments: Vec<String>,
}

pub(crate) struct ResolveTestRunParamsRequestHandler;

impl RequestHandler for ResolveTestRunParamsRequestHandler {
    type RequestType = ResolveTestRunParamsRequest;
}

impl BackgroundDocumentRequestHandler for ResolveTestRunParamsRequestHandler {
    fn document_uri(params: &ResolveTestRunParamsParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        params: ResolveTestRunParamsParams,
    ) -> crate::server::Result<Option<TestRunParams>> {
        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(file_path) = super::discover_tests::test_file_path(db, file) else {
            return Ok(None);
        };

        // Only resolve tests that discovery actually reports for this document. A `null`
        // response tells the client that its test id is unknown or stale (for example
        // because the document was edited) and that it should discover the tests again.
        let known_test = discover_tests(db, file).into_iter().any(|test| {
            super::discover_tests::test_id(&file_path, &test.qualified_name) == params.test_id
        });

        if !known_test {
            tracing::debug!(
                "Cannot resolve run params for unknown test `{}`",
                params.test_id
            );
            return Ok(None);
        }

        let program = Program::get(db)
            .python_executable(db)
            .as_deref()
            .map(ToString::to_string);

        Ok(Some(TestRunParams {
            working_directory: db.project().root(db).to_string(),
            program,
            arguments: vec!["-m".to_string(), "pytest".to_string(), params.test_id],
        }))
    }
}

impl RetriableRequestHandler for ResolveTestRunParamsRequestHandler {}
