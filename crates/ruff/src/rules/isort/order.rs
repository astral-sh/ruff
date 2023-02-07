use std::cmp::Ordering;
use std::collections::BTreeSet;

use itertools::Itertools;
use rustc_hash::FxHashMap;

use super::settings::RelativeImportsOrder;
use super::sorting::{cmp_import_from, cmp_members, cmp_modules};
use super::types::{AliasData, CommentSet, ImportBlock, OrderedImportBlock, TrailingComma};

pub fn order_imports<'a>(
    block: ImportBlock<'a>,
    order_by_type: bool,
    relative_imports_order: RelativeImportsOrder,
    classes: &'a BTreeSet<String>,
    constants: &'a BTreeSet<String>,
    variables: &'a BTreeSet<String>,
) -> OrderedImportBlock<'a> {
    let mut ordered = OrderedImportBlock::default();

    // Sort `StmtKind::Import`.
    ordered.import.extend(
        block
            .import
            .into_iter()
            .sorted_by(|(alias1, _), (alias2, _)| cmp_modules(alias1, alias2)),
    );

    // Sort `StmtKind::ImportFrom`.
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
                    .map(|((import_from, alias), comments)| {
                        (
                            import_from,
                            (
                                CommentSet {
                                    atop: comments.atop,
                                    inline: vec![],
                                },
                                FxHashMap::from_iter([(
                                    alias,
                                    CommentSet {
                                        atop: vec![],
                                        inline: comments.inline,
                                    },
                                )]),
                                TrailingComma::Absent,
                            ),
                        )
                    }),
            )
            .chain(
                // Include all star imports.
                block
                    .import_from_star
                    .into_iter()
                    .map(|(import_from, comments)| {
                        (
                            import_from,
                            (
                                CommentSet {
                                    atop: comments.atop,
                                    inline: vec![],
                                },
                                FxHashMap::from_iter([(
                                    AliasData {
                                        name: "*",
                                        asname: None,
                                    },
                                    CommentSet {
                                        atop: vec![],
                                        inline: comments.inline,
                                    },
                                )]),
                                TrailingComma::Absent,
                            ),
                        )
                    }),
            )
            .map(|(import_from, (comments, aliases, locations))| {
                // Within each `StmtKind::ImportFrom`, sort the members.
                (
                    import_from,
                    comments,
                    locations,
                    aliases
                        .into_iter()
                        .sorted_by(|(alias1, _), (alias2, _)| {
                            cmp_members(
                                alias1,
                                alias2,
                                order_by_type,
                                classes,
                                constants,
                                variables,
                            )
                        })
                        .collect::<Vec<(AliasData, CommentSet)>>(),
                )
            })
            .sorted_by(
                |(import_from1, _, _, aliases1), (import_from2, _, _, aliases2)| {
                    cmp_import_from(import_from1, import_from2, relative_imports_order).then_with(
                        || match (aliases1.first(), aliases2.first()) {
                            (None, None) => Ordering::Equal,
                            (None, Some(_)) => Ordering::Less,
                            (Some(_), None) => Ordering::Greater,
                            (Some((alias1, _)), Some((alias2, _))) => cmp_members(
                                alias1,
                                alias2,
                                order_by_type,
                                classes,
                                constants,
                                variables,
                            ),
                        },
                    )
                },
            ),
    );
    ordered
}
