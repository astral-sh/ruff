use std::cmp::Ordering;
use std::collections::BTreeSet;

use itertools::Itertools;

use crate::rules::isort::types::ImportFromStatement;

use super::settings::RelativeImportsOrder;
use super::sorting::{cmp_import_from, cmp_members, cmp_modules};
use super::types::{AliasData, CommentSet, ImportBlock, OrderedImportBlock};

#[allow(clippy::too_many_arguments)]
pub(crate) fn order_imports<'a>(
    block: ImportBlock<'a>,
    order_by_type: bool,
    case_sensitive: bool,
    relative_imports_order: RelativeImportsOrder,
    classes: &'a BTreeSet<String>,
    constants: &'a BTreeSet<String>,
    variables: &'a BTreeSet<String>,
    force_to_top: &'a BTreeSet<String>,
) -> OrderedImportBlock<'a> {
    let mut ordered = OrderedImportBlock::default();

    // Sort `Stmt::Import`.
    ordered.import.extend(
        block
            .import
            .into_iter()
            .sorted_by(|(alias1, _), (alias2, _)| {
                cmp_modules(alias1, alias2, force_to_top, case_sensitive)
            }),
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
                            .sorted_by(|(alias1, _), (alias2, _)| {
                                cmp_members(
                                    alias1,
                                    alias2,
                                    order_by_type,
                                    classes,
                                    constants,
                                    variables,
                                    force_to_top,
                                    case_sensitive,
                                )
                            })
                            .collect::<Vec<(AliasData, CommentSet)>>(),
                    )
                },
            )
            .sorted_by(
                |(import_from1, _, _, aliases1), (import_from2, _, _, aliases2)| {
                    cmp_import_from(
                        import_from1,
                        import_from2,
                        relative_imports_order,
                        force_to_top,
                        case_sensitive,
                    )
                    .then_with(|| match (aliases1.first(), aliases2.first()) {
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
                            force_to_top,
                            case_sensitive,
                        ),
                    })
                },
            ),
    );
    ordered
}
