//! Utility functions common to language server request handlers
//! that return symbol information.

use lsp_types::{SymbolInformation, SymbolKind};
use ty_ide::SymbolInfo;

use crate::Db;
use crate::document::{PositionEncoding, ToRangeExt};

/// Convert `ty_ide` `SymbolKind` to LSP `SymbolKind`
pub(crate) fn convert_symbol_kind(kind: ty_ide::SymbolKind) -> SymbolKind {
    match kind {
        ty_ide::SymbolKind::Module => SymbolKind::MODULE,
        ty_ide::SymbolKind::Class => SymbolKind::CLASS,
        ty_ide::SymbolKind::Method => SymbolKind::METHOD,
        ty_ide::SymbolKind::Function => SymbolKind::FUNCTION,
        ty_ide::SymbolKind::Variable => SymbolKind::VARIABLE,
        ty_ide::SymbolKind::Constant => SymbolKind::CONSTANT,
        ty_ide::SymbolKind::Property => SymbolKind::PROPERTY,
        ty_ide::SymbolKind::Field => SymbolKind::FIELD,
        ty_ide::SymbolKind::Constructor => SymbolKind::CONSTRUCTOR,
        ty_ide::SymbolKind::Parameter => SymbolKind::VARIABLE,
        ty_ide::SymbolKind::TypeParameter => SymbolKind::TYPE_PARAMETER,
        ty_ide::SymbolKind::Import => SymbolKind::MODULE,
    }
}

/// Convert a `ty_ide` `SymbolInfo` to LSP `SymbolInformation`
///
/// Returns `None` if the symbol's range cannot be converted to a location
/// (e.g., if the file cannot be converted to a URL).
pub(crate) fn convert_to_lsp_symbol_information(
    db: &dyn Db,
    file: ruff_db::files::File,
    symbol: SymbolInfo<'_>,
    encoding: PositionEncoding,
) -> Option<SymbolInformation> {
    let symbol_kind = convert_symbol_kind(symbol.kind);
    let location = symbol
        .full_range
        .to_lsp_range(db, file, encoding)?
        .to_location()?;
    Some(SymbolInformation {
        name: symbol.name.into_owned(),
        kind: symbol_kind,
        tags: None,
        #[allow(deprecated)]
        deprecated: None,
        location,
        container_name: None,
    })
}
