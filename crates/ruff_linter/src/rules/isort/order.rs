use itertools::Itertools;

use super::settings::Settings;
use super::sorting::{MemberKey, ModuleKey};
use super::types::{AliasData, CommentSet, ImportBlock, ImportFromStatement, OrderedImportBlock};

pub(crate) fn order_imports<'a>(
    block: ImportBlock<'a>,
    settings: &Settings,
) -> OrderedImportBlock<'a> {
    let mut ordered = OrderedImportBlock::default();

    // Sort `Stmt::Import`.
    ordered
        .import
        .extend(block.import.into_iter().sorted_by_cached_key(|(alias, _)| {
            ModuleKey::from_module(Some(alias.name), alias.asname, None, None, settings)
        }));

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
                                MemberKey::from_member(alias.name, alias.asname, settings)
                            })
                            .collect::<Vec<(AliasData, CommentSet)>>(),
                    )
                },
            )
            .sorted_by_cached_key(|(import_from, _, _, aliases)| {
                ModuleKey::from_module(
                    import_from.module,
                    None,
                    import_from.level,
                    aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                    settings,
                )
            }),
    );
    ordered
}
