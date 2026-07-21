use lsp_types::{SymbolKind, TypeHierarchyItem};
use ruff_db::{Db as _, PythonFile};
use ty_project::ProjectDatabase;

use crate::PositionEncoding;
use crate::document::{ToRangeExt, resolve_file_uri_range};
use crate::session::SessionSnapshot;
use crate::system::file_to_uri;

#[derive(Debug, Copy, Clone)]
pub(crate) enum TypeHierarchyKind {
    Subtypes,
    Supertypes,
}

/// The subtype and supertype implementation.
pub(crate) fn hierarchy_handler(
    snapshot: &SessionSnapshot,
    requested_item: &TypeHierarchyItem,
    hierarchy_kind: TypeHierarchyKind,
) -> Option<Vec<TypeHierarchyItem>> {
    let encoding = snapshot.position_encoding();

    // We don't actually know which project the request
    // came from, so just look for results across all
    // projects.
    let mut items = vec![];
    for db in snapshot.projects() {
        let Some((file, offset)) = resolve_file_uri_range(
            db,
            &requested_item.uri,
            requested_item.selection_range,
            encoding,
        ) else {
            continue;
        };
        let file = PythonFile::new(db, file, db.python_version());
        let hierarchy_types = match hierarchy_kind {
            TypeHierarchyKind::Subtypes => ty_ide::type_hierarchy_subtypes(db, file, offset),
            TypeHierarchyKind::Supertypes => ty_ide::type_hierarchy_supertypes(db, file, offset),
        };
        items.extend(
            hierarchy_types
                .into_iter()
                .filter_map(|item| convert_to_lsp_item(db, item, encoding)),
        );
    }
    if items.is_empty() { None } else { Some(items) }
}

pub(crate) fn convert_to_lsp_item(
    db: &ProjectDatabase,
    item: ty_ide::TypeHierarchyItem,
    encoding: PositionEncoding,
) -> Option<TypeHierarchyItem> {
    let uri = file_to_uri(db, item.file)?;
    let full_range = item.full_range.to_lsp_range(db, item.file, encoding)?;
    let selection_range = item.selection_range.to_lsp_range(db, item.file, encoding)?;

    Some(TypeHierarchyItem {
        name: item.name.into(),
        kind: SymbolKind::Class,
        tags: None,
        detail: item.detail,
        uri,
        range: full_range.local_range(),
        selection_range: selection_range.local_range(),
        data: None,
    })
}
