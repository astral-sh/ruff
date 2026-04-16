use std::borrow::Cow;

use lsp_types::request::CodeLensRequest;
use lsp_types::{CodeLens, CodeLensParams, Url};
use ty_ide::{CodeLensCommand, code_lens};
use ty_project::{Db as _, ProjectDatabase};
use ty_python_semantic::Program;

use crate::capabilities::SupportedCommand;
use crate::document::ToRangeExt;
use crate::server::api::requests::execute_command::RunTestArgs;
use crate::server::api::traits::{
    BackgroundDocumentRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::DocumentSnapshot;
use crate::session::client::Client;

pub(crate) struct CodeLensRequestHandler;

impl RequestHandler for CodeLensRequestHandler {
    type RequestType = CodeLensRequest;
}

impl BackgroundDocumentRequestHandler for CodeLensRequestHandler {
    fn document_url(params: &CodeLensParams) -> Cow<'_, Url> {
        Cow::Borrowed(&params.text_document.uri)
    }

    fn run_with_snapshot(
        db: &ProjectDatabase,
        snapshot: &DocumentSnapshot,
        _client: &Client,
        _params: CodeLensParams,
    ) -> crate::server::Result<Option<Vec<CodeLens>>> {
        let Some(file) = snapshot.to_notebook_or_file(db) else {
            return Ok(None);
        };

        let root = db.project().root(db);
        let Some(file_path) = file
            .path(db)
            .as_system_path()
            .map(|p| p.strip_prefix(root).unwrap_or(p).to_string())
        else {
            return Ok(None);
        };

        let items = code_lens(db, file);
        let cwd = root.to_string();
        let python_executable = Program::get(db).python_executable(db);

        let lenses: Vec<CodeLens> = items
            .into_iter()
            .filter_map(|item| {
                let range = item.range.to_lsp_range(db, file, snapshot.encoding())?;

                let args = match &item.command {
                    CodeLensCommand::RunTest { test } => {
                        let run_test_args =
                            RunTestArgs::new(&cwd, &file_path, test, python_executable.as_deref());
                        serde_json::to_value(&run_test_args).ok()?
                    }
                };

                Some(CodeLens {
                    range: range.local_range(),
                    command: Some(lsp_types::Command {
                        title: item.title,
                        command: SupportedCommand::RunTest.identifier().to_string(),
                        arguments: Some(vec![args]),
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
