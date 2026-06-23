use std::collections::HashMap;

use lsp_types::{
    DocumentChange, OptionalVersionedTextDocumentIdentifier, RenameFilesParams, TextDocumentEdit,
    TextDocumentIdentifier, TextEdit, Uri, WillRenameFilesRequest, WorkspaceEdit,
};
use ruff_db::system::SystemPathBuf;
use ty_ide::{will_rename_directory, will_rename_file};

use crate::document::ToRangeExt;
use crate::server::api::traits::{
    BackgroundRequestHandler, RequestHandler, RetriableRequestHandler,
};
use crate::session::SessionSnapshot;
use crate::session::client::Client;
use crate::system::file_to_uri;

pub(crate) struct WillRenameFilesHandler;

impl RequestHandler for WillRenameFilesHandler {
    type RequestType = WillRenameFilesRequest;
}

impl BackgroundRequestHandler for WillRenameFilesHandler {
    fn run(
        snapshot: &SessionSnapshot,
        _client: &Client,
        params: RenameFilesParams,
    ) -> crate::server::Result<Option<WorkspaceEdit>> {
        let encoding = snapshot.position_encoding();
        let mut all_changes: HashMap<Uri, Vec<TextEdit>> = HashMap::new();

        for file_rename in &params.files {
            let Ok(old_uri) = Uri::parse(&file_rename.old_uri) else {
                continue;
            };
            let Ok(new_uri) = Uri::parse(&file_rename.new_uri) else {
                continue;
            };

            let Ok(old_std_path) = old_uri.to_file_path() else {
                continue;
            };
            let Ok(new_std_path) = new_uri.to_file_path() else {
                continue;
            };

            let Ok(old_path) = SystemPathBuf::from_path_buf(old_std_path) else {
                continue;
            };
            let Ok(new_path) = SystemPathBuf::from_path_buf(new_std_path) else {
                continue;
            };

            let has_python_extension = matches!(old_path.extension(), Some("py" | "pyi"));

            for db in snapshot.projects() {
                // Paths with a Python extension are always files.
                // Everything else is treated as a directory (package).
                // Both functions gracefully return empty results on mismatch.
                let rename_edits = if has_python_extension {
                    will_rename_file(db, &old_path, &new_path)
                } else {
                    will_rename_directory(db, &old_path, &new_path)
                };

                for edit in rename_edits {
                    let Some(uri) = file_to_uri(db, edit.file) else {
                        continue;
                    };

                    let Some(lsp_range) = edit.range.to_lsp_range(db, edit.file, encoding) else {
                        continue;
                    };

                    all_changes.entry(uri).or_default().push(TextEdit {
                        range: lsp_range.local_range(),
                        new_text: edit.new_text,
                    });
                }
            }
        }

        if all_changes.is_empty() {
            Ok(None)
        } else {
            let document_changes: Vec<DocumentChange> = all_changes
                .into_iter()
                .map(|(uri, edits)| {
                    DocumentChange::TextDocumentEdit(TextDocumentEdit {
                        text_document: OptionalVersionedTextDocumentIdentifier {
                            text_document_identifier: TextDocumentIdentifier { uri },
                            version: None,
                        },
                        edits: edits.into_iter().map(lsp_types::Edit::TextEdit).collect(),
                    })
                })
                .collect();

            Ok(Some(WorkspaceEdit {
                document_changes: Some(document_changes),
                ..Default::default()
            }))
        }
    }
}

impl RetriableRequestHandler for WillRenameFilesHandler {}
