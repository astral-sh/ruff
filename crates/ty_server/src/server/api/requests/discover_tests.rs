use std::borrow::Cow;

use lsp_types::{LspRequestMethod, MessageDirection, Range, Request, TextDocumentIdentifier, Uri};
use ruff_db::files::File;
use serde::{Deserialize, Serialize};
use ty_ide::{TestKind, discover_tests};
use ty_project::{Db as _, ProjectDatabase};

use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

/// Custom `ty/discoverTests` request that lists the tests defined in a document.
///
/// Clients that implement their own test-runner integration (such as the VS Code test
/// explorer or neotest) use this to build the test tree, instead of relying on code
/// lenses. The response only describes the tests; how to run one can be resolved with
/// the `ty/resolveTestRunParams` request.
pub(crate) enum DiscoverTestsRequest {}

impl Request for DiscoverTestsRequest {
    type Params = DiscoverTestsParams;
    type Result = Vec<DiscoveredTest>;
    const METHOD: LspRequestMethod<'static> = LspRequestMethod::Custom("ty/discoverTests");
    const MESSAGE_DIRECTION: MessageDirection = MessageDirection::ClientToServer;
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoverTestsParams {
    pub(crate) text_document: TextDocumentIdentifier,
}

/// A test discovered in a document.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscoveredTest {
    /// Identifies the test within its project: the path of the file relative to the
    /// project root followed by the `::`-separated path of the test within the file,
    /// e.g. `tests/test_foo.py::TestFoo::test_bar`. This is also a valid pytest
    /// selector for the test.
    pub(crate) id: String,
    /// The display name of the test, e.g. `test_bar`.
    pub(crate) label: String,
    pub(crate) kind: DiscoveredTestKind,
    /// The range of the test's name in the document.
    pub(crate) range: Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DiscoveredTestKind {
    /// A test function, either at module level or inside a test class.
    Function,
    /// A class that groups test functions.
    Class,
}

impl From<TestKind> for DiscoveredTestKind {
    fn from(kind: TestKind) -> Self {
        match kind {
            TestKind::Function => DiscoveredTestKind::Function,
            TestKind::Class => DiscoveredTestKind::Class,
        }
    }
}

/// Returns the path of `file` relative to the project root, which is the file part of a
/// test id and the path pytest selectors are expected to use when run from the project
/// root.
pub(crate) fn test_file_path(db: &ProjectDatabase, file: File) -> Option<String> {
    let root = db.project().root(db);
    file.path(db)
        .as_system_path()
        .map(|path| path.strip_prefix(root).unwrap_or(path).to_string())
}

/// Builds a test id from the file part and the qualified name of the test within the
/// file, e.g. `tests/test_foo.py` and `TestFoo::test_bar` become
/// `tests/test_foo.py::TestFoo::test_bar`.
pub(crate) fn test_id(file_path: &str, qualified_name: &str) -> String {
    format!("{file_path}::{qualified_name}")
}

pub(crate) struct DiscoverTestsRequestHandler;

impl RequestHandler for DiscoverTestsRequestHandler {
    type RequestType = DiscoverTestsRequest;
}

impl BackgroundDocumentRequestHandler for DiscoverTestsRequestHandler {
    fn document_uri(params: &DiscoverTestsParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        _params: DiscoverTestsParams,
    ) -> crate::server::Result<Vec<DiscoveredTest>> {
        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(Vec::new());
        };

        let Some(file_path) = test_file_path(db, file) else {
            return Ok(Vec::new());
        };

        let tests = discover_tests(db, file)
            .into_iter()
            .filter_map(|test| {
                let range = test.range.to_lsp_range(db, file, snapshot.encoding())?;

                Some(DiscoveredTest {
                    id: test_id(&file_path, &test.qualified_name),
                    label: test.label,
                    kind: test.kind.into(),
                    range: range.local_range(),
                })
            })
            .collect();

        Ok(tests)
    }
}

impl RetriableRequestHandler for DiscoverTestsRequestHandler {}
