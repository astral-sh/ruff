/*!
This module provides the trait implementations necessary to implement each of
the LSP request handlers.

Every request handler should live in its own module, with a module name
matching the LSP protocol request name as closely as possible. This should be
done even when there is tight coupling between multiple request handlers (like
type hierarchy) to make it easy to continue to find the right handler given
knowledge about the request name.

If request handlers need shared helper functions, they can go in a sibling
module. For example, see `super::type_hierarchy`.
*/

mod code_action;
mod completion;
mod diagnostic;
mod doc_highlights;
mod document_symbols;
mod execute_command;
mod folding_range;
mod goto_declaration;
mod goto_definition;
mod goto_type_definition;
mod hover;
mod inlay_hints;
mod prepare_rename;
mod prepare_type_hierarchy;
mod references;
mod rename;
mod selection_range;
mod semantic_tokens;
mod semantic_tokens_range;
mod shutdown;
mod signature_help;
mod type_hierarchy_subtypes;
mod type_hierarchy_supertypes;
mod workspace_diagnostic;
mod workspace_symbols;

pub(super) use code_action::CodeActionRequestHandler;
pub(super) use completion::CompletionRequestHandler;
pub(super) use diagnostic::DocumentDiagnosticRequestHandler;
pub(super) use doc_highlights::DocumentHighlightRequestHandler;
pub(super) use document_symbols::DocumentSymbolRequestHandler;
pub(super) use execute_command::ExecuteCommand;
pub(super) use folding_range::FoldingRangeRequestHandler;
pub(super) use goto_declaration::GotoDeclarationRequestHandler;
pub(super) use goto_definition::GotoDefinitionRequestHandler;
pub(super) use goto_type_definition::GotoTypeDefinitionRequestHandler;
pub(super) use hover::HoverRequestHandler;
pub(super) use inlay_hints::InlayHintRequestHandler;
pub(super) use prepare_rename::PrepareRenameRequestHandler;
pub(super) use prepare_type_hierarchy::PrepareTypeHierarchyRequestHandler;
pub(super) use references::ReferencesRequestHandler;
pub(super) use rename::RenameRequestHandler;
pub(super) use selection_range::SelectionRangeRequestHandler;
pub(super) use semantic_tokens::SemanticTokensRequestHandler;
pub(super) use semantic_tokens_range::SemanticTokensRangeRequestHandler;
pub(super) use shutdown::ShutdownHandler;
pub(super) use signature_help::SignatureHelpRequestHandler;
pub(super) use type_hierarchy_subtypes::TypeHierarchySubtypesRequestHandler;
pub(super) use type_hierarchy_supertypes::TypeHierarchySupertypesRequestHandler;
pub(super) use workspace_diagnostic::WorkspaceDiagnosticRequestHandler;
pub(super) use workspace_symbols::WorkspaceSymbolRequestHandler;

pub use workspace_diagnostic::{PartialWorkspaceProgress, PartialWorkspaceProgressParams};
