use itertools::Itertools;

use super::settings::{RelativeImportsOrder, Settings};
use super::sorting::{member_key, module_key};
use super::types::{AliasData, CommentSet, ImportBlock, ImportFromStatement, OrderedImportBlock};

pub(crate) fn order_imports<'a>(
    block: ImportBlock<'a>,
    settings: &Settings,
) -> OrderedImportBlock<'a> {
    let mut ordered = OrderedImportBlock::default();

    // Sort `Stmt::Import`.
    ordered.import.extend(
        block
            .import
            .into_iter()
            .sorted_by_cached_key(|(alias, _)| module_key(alias.name, alias.asname, settings)),
    );

    // Sort `Stmt::ImportFrom`.
    ordered.import_from.extend(
        // Include all non-re-exports.
        block
            .import_from
            .into_iter()
            .chain(
                // Include all re-exports.
                block
                    .import_from_as
                    .into_iter()
                    .map(|((import_from, ..), body)| (import_from, body)),
            )
            .chain(
                // Include all star imports.
                block.import_from_star,
            )
            .map(
                |(
                    import_from,
                    ImportFromStatement {
                        comments,
                        aliases,
                        trailing_comma,
                    },
                )| {
                    // Within each `Stmt::ImportFrom`, sort the members.
                    (
                        import_from,
                        comments,
                        trailing_comma,
                        aliases
                            .into_iter()
                            .sorted_by_cached_key(|(alias, _)| {
                                member_key(alias.name, alias.asname, settings)
                            })
                            .collect::<Vec<(AliasData, CommentSet)>>(),
                    )
                },
            )
            .sorted_by_cached_key(|(import_from, _, _, aliases)| {
                (
                    import_from.level.map(i64::from).map(|l| {
                        match settings.relative_imports_order {
                            RelativeImportsOrder::ClosestToFurthest => l,
                            RelativeImportsOrder::FurthestToClosest => -l,
                        }
                    }),
                    import_from
                        .module
                        .as_ref()
                        .map(|module| module_key(module, None, settings)),
                    aliases
                        .first()
                        .map(|(alias, _)| member_key(alias.name, alias.asname, settings)),
                )
            }),
    );
    ordered
}
