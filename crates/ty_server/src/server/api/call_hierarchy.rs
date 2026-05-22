//! Shared helpers for the LSP call hierarchy request handlers.
//!
//! Mirrors the layout of `super::type_hierarchy` because the two LSP features
//! are structurally identical: a stateless `prepare` followed by directional
//! queries that round-trip the symbol identity via the item's `uri` and
//! `selection_range.start`.

use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyItem, CallHierarchyOutgoingCall, SymbolKind,
};
use ruff_db::files::{File, system_path_to_file, vendored_path_to_file};
use ruff_db::system::SystemPathBuf;
use ruff_text_size::TextSize;
use ty_project::ProjectDatabase;

use crate::PositionEncoding;
use crate::document::{PositionExt, ToRangeExt};
use crate::session::SessionSnapshot;
use crate::system::file_to_url;

/// Helper for `callHierarchy/incomingCalls`. Iterates over all projects in the
/// session and merges results from whichever project owns the file.
pub(crate) fn incoming_calls_handler(
    snapshot: &SessionSnapshot,
    requested_item: &CallHierarchyItem,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let encoding = snapshot.position_encoding();
    let mut calls = Vec::new();
    for db in snapshot.projects() {
        let Some((file, offset)) = resolve_item_location(db, requested_item, encoding) else {
            continue;
        };
        for call in ty_ide::call_hierarchy_incoming_calls(db, file, offset) {
            // `from_ranges` are byte offsets into `call.from.file` (the caller),
            // NOT into `file` (the prepared/queried symbol). Capture the caller
            // file before moving `call.from` into `convert_to_lsp_item`.
            let caller_file = call.from.file;
            let Some(from) = convert_to_lsp_item(db, call.from, encoding) else {
                continue;
            };
            let from_ranges: Vec<_> = call
                .from_ranges
                .into_iter()
                .filter_map(|range| range.to_lsp_range(db, caller_file, encoding))
                .map(|file_range| file_range.local_range())
                .collect();
            calls.push(CallHierarchyIncomingCall { from, from_ranges });
        }
    }
    if calls.is_empty() { None } else { Some(calls) }
}

/// Helper for `callHierarchy/outgoingCalls`. Same multi-project iteration as
/// the incoming variant, but `from_ranges` come from the *prepared* item's
/// file (the caller).
pub(crate) fn outgoing_calls_handler(
    snapshot: &SessionSnapshot,
    requested_item: &CallHierarchyItem,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let encoding = snapshot.position_encoding();
    let mut calls = Vec::new();
    for db in snapshot.projects() {
        let Some((file, offset)) = resolve_item_location(db, requested_item, encoding) else {
            continue;
        };
        for call in ty_ide::call_hierarchy_outgoing_calls(db, file, offset) {
            let Some(to) = convert_to_lsp_item(db, call.to, encoding) else {
                continue;
            };
            let from_ranges: Vec<_> = call
                .from_ranges
                .into_iter()
                .filter_map(|range| range.to_lsp_range(db, file, encoding))
                .map(|file_range| file_range.local_range())
                .collect();
            calls.push(CallHierarchyOutgoingCall { to, from_ranges });
        }
    }
    if calls.is_empty() { None } else { Some(calls) }
}

/// Re-derive `(File, TextSize)` from a `CallHierarchyItem` so the follow-up
/// request can call back into `ty_ide`. The symbol identity is encoded via
/// the item's `(uri, selection_range.start)` — there is intentionally no
/// reliance on the opaque `data` field.
// TODO: duplicates similar function `super::type_hierarchy::resolve_item_location`
//        Consider lifting to a shared utility module.
pub(crate) fn resolve_item_location(
    db: &ProjectDatabase,
    item: &CallHierarchyItem,
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
                    "Could not resolve call hierarchy item location \
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
                    "Could not resolve call hierarchy item location \
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

// TODO: shares boilerplate with `super::type_hierarchy::convert_to_lsp_item`
//       (call hierarchy additionally maps a kind enum). Consider lifting
//       to a shared utility.
pub(crate) fn convert_to_lsp_item(
    db: &ProjectDatabase,
    item: ty_ide::CallHierarchyItem,
    encoding: PositionEncoding,
) -> Option<CallHierarchyItem> {
    let uri = file_to_url(db, item.file)?;
    let full_range = item.full_range.to_lsp_range(db, item.file, encoding)?;
    let selection_range = item.selection_range.to_lsp_range(db, item.file, encoding)?;

    let kind = match item.kind {
        ty_ide::CallHierarchyItemKind::Function => SymbolKind::FUNCTION,
        ty_ide::CallHierarchyItemKind::Method => SymbolKind::METHOD,
        ty_ide::CallHierarchyItemKind::Class => SymbolKind::CLASS,
        ty_ide::CallHierarchyItemKind::Module => SymbolKind::MODULE,
    };

    Some(CallHierarchyItem {
        name: item.name.into(),
        kind,
        tags: None,
        detail: item.detail,
        uri,
        range: full_range.local_range(),
        selection_range: selection_range.local_range(),
        // The `data` field is intentionally unused. We re-derive identity from
        // `(uri, selection_range.start)` — see `resolve_item_location`.
        data: None,
    })
}
