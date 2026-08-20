use std::borrow::Cow;

use lsp_types::CodeLensRequest;
use lsp_types::{CodeLens, CodeLensParams, Uri};
use serde::{Deserialize, Serialize};
use ty_ide::{TestKind, discover_tests};
use ty_project::ProjectDatabase;

use crate::document::ToRangeExt;
use crate::server::api::requests::discover_tests::{DiscoveredTestKind, test_file_path, test_id};
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

/// The command attached to test code lenses.
/// To resolve the command to run the test `ty/resolveTestRunParams` must be used.
pub(crate) const RUN_TEST_COMMAND: &str = "ty.runTest";

/// Arguments for [`RUN_TEST_COMMAND`] code lens command.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RunTestCommandArgs {
    /// The document the test was discovered in.
    pub(crate) uri: Uri,
    /// The id of the test, as also returned by `ty/discoverTests`,
    /// e.g. `tests/test_foo.py::TestFoo::test_bar`.
    pub(crate) test_id: String,
    /// The display name of the test, e.g. `test_bar`.
    pub(crate) label: String,
    pub(crate) kind: DiscoveredTestKind,
}

pub(crate) struct CodeLensRequestHandler;

impl RequestHandler for CodeLensRequestHandler {
    type RequestType = CodeLensRequest;
}

impl BackgroundDocumentRequestHandler for CodeLensRequestHandler {
    fn document_uri(params: &CodeLensParams) -> Cow<'_, Uri> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        _params: CodeLensParams,
    ) -> crate::server::Result<Option<Vec<CodeLens>>> {
        if !snapshot.resolved_client_capabilities().supports_run_tests() {
            return Ok(None);
        }

        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let Some(file_path) = test_file_path(db, file) else {
            return Ok(None);
        };

        let lenses: Vec<CodeLens> = discover_tests(db, file)
            .into_iter()
            .filter_map(|test| {
                let range = test.range.to_lsp_range(db, file, snapshot.encoding())?;

                let title = match test.kind {
                    TestKind::Function => "Run test",
                    TestKind::Class => "Run tests",
                };

                let args = RunTestCommandArgs {
                    uri: snapshot.uri().clone(),
                    test_id: test_id(&file_path, &test.qualified_name),
                    label: test.label,
                    kind: test.kind.into(),
                };

                Some(CodeLens {
                    range: range.local_range(),
                    command: Some(lsp_types::Command {
                        title: title.to_string(),
                        command: RUN_TEST_COMMAND.to_string(),
                        arguments: Some(vec![serde_json::to_value(args).ok()?]),
                        ..Default::default()
                    }),
                    data: None,
                })
            })
            .collect();

        if lenses.is_empty() {
            Ok(None)
        } else {
            Ok(Some(lenses))
        }
    }
}

impl RetriableRequestHandler for CodeLensRequestHandler {}
