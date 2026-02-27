use lsp_types::{SymbolKind, TypeHierarchyItem};
use ruff_db::files::{File, system_path_to_file, vendored_path_to_file};
use ruff_db::system::SystemPathBuf;
use ruff_text_size::TextSize;
use ty_project::ProjectDatabase;

use crate::PositionEncoding;
use crate::document::{PositionExt, ToRangeExt};
use crate::session::SessionSnapshot;
use crate::system::file_to_url;

/// The subtype and supertype implementation.
///
/// `hierarchy_types` should be either `ty_ide::type_hierarchy_subtypes`
/// or `ty_ide::type_hierarchy_supertypes`.
pub(crate) fn hierarchy_handler(
    snapshot: &SessionSnapshot,
    requested_item: &TypeHierarchyItem,
    hierarchy_types: fn(&dyn ty_project::Db, File, TextSize) -> Vec<ty_ide::TypeHierarchyItem>,
) -> Option<Vec<TypeHierarchyItem>> {
    let encoding = snapshot.position_encoding();

    // We don't actually know which project the request
    // came from, so just look for results across all
    // projects.
    let mut items = vec![];
    for db in snapshot.projects() {
        let Some((file, offset)) = resolve_item_location(db, requested_item, encoding) else {
            continue;
        };
        items.extend(
            hierarchy_types(db, file, offset)
                .into_iter()
                .filter_map(|item| convert_to_lsp_item(db, item, encoding)),
        );
    }
    if items.is_empty() { None } else { Some(items) }
}

/// Attempts to resolve the location in the provided
/// type hierarchy item into `ty_ide` types. This includes
/// mapping system paths back into their proper vendored
/// path types (if applicable).
fn resolve_item_location(
    db: &ProjectDatabase,
    item: &TypeHierarchyItem,
    encoding: PositionEncoding,
) -> Option<(File, TextSize)> {
    let system_path = SystemPathBuf::from_path_buf(item.uri.to_file_path().ok()?).ok()?;

    let file = if let Some(ref vendored_root) = ty_ide::cached_vendored_root(db)
        && let Some(vendored_path) = ty_ide::map_system_to_vendored(vendored_root, &system_path)
    {
        match vendored_path_to_file(db, vendored_path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(
                    "Could not resolve type hierarchy item location \
                     for vendored file path `{vendored_path}`: {err}"
                );
                return None;
            }
        }
    } else {
        match system_path_to_file(db, &system_path) {
            Ok(file) => file,
            Err(err) => {
                tracing::warn!(
                    "Could not resolve type hierarchy item location \
                     for system file path `{system_path}`: {err}"
                );
                return None;
            }
        }
    };

    let offset = item
        .selection_range
        .start
        .to_text_size(db, file, &item.uri, encoding)?;
    Some((file, offset))
}

pub(crate) fn convert_to_lsp_item(
    db: &ProjectDatabase,
    item: ty_ide::TypeHierarchyItem,
    encoding: PositionEncoding,
) -> Option<TypeHierarchyItem> {
    let uri = file_to_url(db, item.file)?;
    let full_range = item.full_range.to_lsp_range(db, item.file, encoding)?;
    let selection_range = item.selection_range.to_lsp_range(db, item.file, encoding)?;

    Some(TypeHierarchyItem {
        name: item.name.into(),
        kind: SymbolKind::CLASS,
        tags: None,
        detail: item.detail,
        uri,
        range: full_range.local_range(),
        selection_range: selection_range.local_range(),
        data: None,
    })
}
