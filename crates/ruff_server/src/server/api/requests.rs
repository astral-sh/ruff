mod code_action;
mod code_action_resolve;
mod diagnostic;
mod format;
mod format_range;

use super::{
    define_document_url,
    traits::{BackgroundDocumentRequestHandler, RequestHandler},
};
pub(super) use code_action::CodeActions;
pub(super) use code_action_resolve::CodeActionResolve;
pub(super) use diagnostic::DocumentDiagnostic;
pub(super) use format::Format;
pub(super) use format_range::FormatRange;

type FormatResponse = Option<Vec<lsp_types::TextEdit>>;
