use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::{LSPObject, Url as Uri};

pub use notification_params::*;

/// A notebook document.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookDocument {
    /// The notebook document's URI.
    pub uri: Uri,
    /// The type of the notebook.
    pub notebook_type: String,
    /// The version number of this document (it will increase after each
    /// change, including undo/redo).
    pub version: i32,
    /// Additional metadata stored with the notebook
    /// document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<LSPObject>,
    /// The cells of a notebook.
    pub cells: Vec<NotebookCell>,
}

/// A notebook cell.
///
/// A cell's document URI must be unique across ALL notebook
/// cells and can therefore be used to uniquely identify a
/// notebook cell or the cell's text document.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookCell {
    /// The cell's kind
    pub kind: NotebookCellKind,
    /// The URI of the cell's text document content.
    pub document: Uri,
    /// Additional metadata stored with the cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<LSPObject>,
    /// Additional execution summary information
    /// if supported by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_summary: Option<ExecutionSummary>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionSummary {
    /// A strict monotonically increasing value
    /// indicating the execution order of a cell
    /// inside a notebook.
    pub execution_order: u32,
    /// Whether the execution was successful or
    /// not if known by the client.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum NotebookCellKind {
    /// A markup-cell is formatted source that is used for display.
    Markup = 1,
    /// A code-cell is source code.
    Code = 2,
}

/// Capabilities specific to the notebook document support.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookDocumentClientCapabilities {
    /// Capabilities specific to notebook document synchronization
    ///
    /// @since 3.17.0
    pub synchronization: NotebookDocumentSyncClientCapabilities,
}

/// Notebook specific client capabilities.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookDocumentSyncClientCapabilities {
    /// Whether implementation supports dynamic registration. If this is
    /// set to `true` the client supports the new
    /// `(TextDocumentRegistrationOptions & StaticRegistrationOptions)`
    /// return value for the corresponding server capability as well.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_registration: Option<bool>,

    /// The client supports sending execution summary data per cell.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_summary_report: Option<bool>,
}

///  Options specific to a notebook plus its cells
///  to be synced to the server.
///
///  If a selector provides a notebook document
///  filter but no cell selector all cells of a
///  matching notebook document will be synced.
///
///  If a selector provides no notebook document
///  filter but only a cell selector all notebook
///  documents that contain at least one matching
///  cell will be synced.
///
///  @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookDocumentSyncOptions {
    /// The notebooks to be synced
    pub notebook_selector: Vec<NotebookSelector>,
    /// Whether save notification should be forwarded to
    /// the server. Will only be honored if mode === `notebook`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save: Option<bool>,
}

/// Registration options specific to a notebook.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookDocumentSyncRegistrationOptions {
    /// The notebooks to be synced
    pub notebook_selector: Vec<NotebookSelector>,
    /// Whether save notification should be forwarded to
    /// the server. Will only be honored if mode === `notebook`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub save: Option<bool>,
    /// The id used to register the request. The id can be used to deregister
    /// the request again. See also Registration#id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// A notebook cell text document filter denotes a cell text
/// document by different properties.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookCellTextDocumentFilter {
    /// A filter that matches against the notebook
    /// containing the notebook cell. If a string
    /// value is provided it matches against the
    /// notebook type. '*' matches every notebook.
    pub notebook: Notebook,
    /// A language id like `python`.
    ///
    /// Will be matched against the language id of the
    /// notebook cell document. '*' matches every language.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum NotebookSelector {
    ByNotebook {
        /// The notebook to be synced. If a string
        /// value is provided it matches against the
        /// notebook type. '*' matches every notebook.
        notebook: Notebook,
        /// The cells of the matching notebook to be synced.
        #[serde(skip_serializing_if = "Option::is_none")]
        cells: Option<Vec<NotebookCellSelector>>,
    },
    ByCells {
        /// The notebook to be synced. If a string
        /// value is provided it matches against the
        /// notebook type. '*' matches every notebook.
        #[serde(skip_serializing_if = "Option::is_none")]
        notebook: Option<Notebook>,
        /// The cells of the matching notebook to be synced.
        cells: Vec<NotebookCellSelector>,
    },
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookCellSelector {
    pub language: String,
}

#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Notebook {
    String(String),
    NotebookDocumentFilter(NotebookDocumentFilter),
}

/// A notebook document filter denotes a notebook document by
/// different properties.
///
/// @since 3.17.0
#[derive(Debug, Eq, PartialEq, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", untagged)]
pub enum NotebookDocumentFilter {
    ByType {
        /// The type of the enclosing notebook.
        notebook_type: String,
        /// A Uri [scheme](#Uri.scheme), like `file` or `untitled`.
        #[serde(skip_serializing_if = "Option::is_none")]
        scheme: Option<String>,
        /// A glob pattern.
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
    ByScheme {
        /// The type of the enclosing notebook.
        #[serde(skip_serializing_if = "Option::is_none")]
        notebook_type: Option<String>,
        /// A Uri [scheme](#Uri.scheme), like `file` or `untitled`.
        scheme: String,
        /// A glob pattern.
        #[serde(skip_serializing_if = "Option::is_none")]
        pattern: Option<String>,
    },
    ByPattern {
        /// The type of the enclosing notebook.
        #[serde(skip_serializing_if = "Option::is_none")]
        notebook_type: Option<String>,
        /// A Uri [scheme](#Uri.scheme), like `file` or `untitled`.
        #[serde(skip_serializing_if = "Option::is_none")]
        scheme: Option<String>,
        /// A glob pattern.
        pattern: String,
    },
}

mod notification_params {
    use serde::{Deserialize, Serialize};

    use crate::{
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
        VersionedTextDocumentIdentifier,
    };

    use super::*;

    /// The params sent in an open notebook document notification.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DidOpenNotebookDocumentParams {
        /// The notebook document that got opened.
        pub notebook_document: NotebookDocument,
        /// The text documents that represent the content
        /// of a notebook cell.
        pub cell_text_documents: Vec<TextDocumentItem>,
    }

    /// The params sent in a change notebook document notification.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DidChangeNotebookDocumentParams {
        /// The notebook document that did change. The version number points
        /// to the version after all provided changes have been applied.
        pub notebook_document: VersionedNotebookDocumentIdentifier,

        /// The actual changes to the notebook document.
        ///
        /// The change describes single state change to the notebook document.
        /// So it moves a notebook document, its cells and its cell text document
        /// contents from state S to S'.
        ///
        /// To mirror the content of a notebook using change events use the
        /// following approach:
        /// - start with the same initial content
        /// - apply the 'notebookDocument/didChange' notifications in the order
        ///   you receive them.
        pub change: NotebookDocumentChangeEvent,
    }

    /// A versioned notebook document identifier.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct VersionedNotebookDocumentIdentifier {
        /// The version number of this notebook document.
        pub version: i32,
        /// The notebook document's URI.
        pub uri: Uri,
    }

    /// A change event for a notebook document.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotebookDocumentChangeEvent {
        /// The changed meta data if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub metadata: Option<LSPObject>,

        /// Changes to cells
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cells: Option<NotebookDocumentCellChange>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotebookDocumentCellChange {
        /// Changes to the cell structure to add or
        /// remove cells.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub structure: Option<NotebookDocumentCellChangeStructure>,

        /// Changes to notebook cells properties like its
        /// kind, execution summary or metadata.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub data: Option<Vec<NotebookCell>>,

        /// Changes to the text content of notebook cells.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub text_content: Option<Vec<NotebookDocumentChangeTextContent>>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotebookDocumentChangeTextContent {
        pub document: VersionedTextDocumentIdentifier,
        pub changes: Vec<TextDocumentContentChangeEvent>,
    }

    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotebookDocumentCellChangeStructure {
        /// The change to the cell array.
        pub array: NotebookCellArrayChange,
        /// Additional opened cell text documents.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub did_open: Option<Vec<TextDocumentItem>>,
        /// Additional closed cell text documents.
        #[serde(skip_serializing_if = "Option::is_none")]
        pub did_close: Option<Vec<TextDocumentIdentifier>>,
    }

    /// A change describing how to move a `NotebookCell`
    /// array from state S to S'.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotebookCellArrayChange {
        /// The start offset of the cell that changed.
        pub start: u32,

        /// The deleted cells
        pub delete_count: u32,

        /// The new cells, if any
        #[serde(skip_serializing_if = "Option::is_none")]
        pub cells: Option<Vec<NotebookCell>>,
    }

    /// The params sent in a save notebook document notification.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DidSaveNotebookDocumentParams {
        /// The notebook document that got saved.
        pub notebook_document: NotebookDocumentIdentifier,
    }

    /// A literal to identify a notebook document in the client.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct NotebookDocumentIdentifier {
        /// The notebook document's URI.
        pub uri: Uri,
    }

    /// The params sent in a close notebook document notification.
    ///
    /// @since 3.17.0
    #[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct DidCloseNotebookDocumentParams {
        /// The notebook document that got closed.
        pub notebook_document: NotebookDocumentIdentifier,

        /// The text documents that represent the content
        /// of a notebook cell that got closed.
        pub cell_text_documents: Vec<TextDocumentIdentifier>,
    }
}
