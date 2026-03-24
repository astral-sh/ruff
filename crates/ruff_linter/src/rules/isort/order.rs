use crate::rules::isort::sorting::ImportStyle;
use crate::rules::isort::{ImportSection, ImportType};
use itertools::Itertools;

use super::settings::Settings;
use super::sorting::{MemberKey, ModuleKey};
use super::types::EitherImport::{self, Import, ImportFrom};
use super::types::{AliasData, ImportBlock, ImportFromCommentSet, ImportFromStatement};

pub(crate) fn order_imports<'a>(
    block: ImportBlock<'a>,
    section: &ImportSection,
    settings: &Settings,
) -> Vec<EitherImport<'a>> {
    let straight_imports = block.import.into_iter();

    let from_imports =
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
                        first_index,
                        comments,
                        aliases,
                        trailing_comma,
                    },
                )| {
                    // Within each `Stmt::ImportFrom`, sort the members.
                    (
                        import_from,
                        first_index.unwrap_or_default(),
                        comments,
                        trailing_comma,
                        aliases
                            .into_iter()
                            .sorted_by_cached_key(|(alias, _)| {
                                MemberKey::from_member(alias.name, alias.asname, settings)
                            })
                            .collect::<Vec<(AliasData, ImportFromCommentSet)>>(),
                    )
                },
            );

    if matches!(section, ImportSection::Known(ImportType::Future)) {
        let ordered_from_imports = from_imports
            .sorted_by_cached_key(|(import_from, first_index, _, _, aliases)| {
                (
                    ModuleKey::from_module(
                        import_from.module,
                        None,
                        import_from.level,
                        aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                        ImportStyle::From,
                        settings,
                    ),
                    *first_index,
                )
            })
            .collect::<Vec<_>>();
        let mut eager_from_imports = vec![];
        let mut lazy_from_imports = vec![];
        for import_from in ordered_from_imports {
            if import_from.0.is_lazy {
                lazy_from_imports.push(import_from);
            } else {
                eager_from_imports.push(import_from);
            }
        }

        let ordered_straight_imports = straight_imports
            .sorted_by_cached_key(|(alias, comments)| {
                (
                    ModuleKey::from_module(
                        Some(alias.name),
                        alias.asname,
                        0,
                        None,
                        ImportStyle::Straight,
                        settings,
                    ),
                    comments.first_index.unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        let mut eager_straight_imports = vec![];
        let mut lazy_straight_imports = vec![];
        for import in ordered_straight_imports {
            if import.0.is_lazy {
                lazy_straight_imports.push(import);
            } else {
                eager_straight_imports.push(import);
            }
        }

        eager_from_imports
            .into_iter()
            .map(|(import_from, _, comments, trailing_comma, aliases)| {
                ImportFrom((import_from, comments, trailing_comma, aliases))
            })
            .chain(eager_straight_imports.into_iter().map(Import))
            .chain(lazy_from_imports.into_iter().map(
                |(import_from, _, comments, trailing_comma, aliases)| {
                    ImportFrom((import_from, comments, trailing_comma, aliases))
                },
            ))
            .chain(lazy_straight_imports.into_iter().map(Import))
            .collect()
    } else if settings.force_sort_within_sections {
        let ordered_imports = straight_imports
            .map(|(alias, comments)| {
                let first_index = comments.first_index.unwrap_or_default();
                (Import((alias, comments)), first_index)
            })
            .chain(from_imports.map(
                |(import_from, first_index, comments, trailing_comma, aliases)| {
                    (
                        ImportFrom((import_from, comments, trailing_comma, aliases)),
                        first_index,
                    )
                },
            ))
            .sorted_by_cached_key(|(import, first_index)| match import {
                Import((alias, _)) => (
                    ModuleKey::from_module(
                        Some(alias.name),
                        alias.asname,
                        0,
                        None,
                        ImportStyle::Straight,
                        settings,
                    ),
                    *first_index,
                ),
                ImportFrom((import_from, _, _, aliases)) => (
                    ModuleKey::from_module(
                        import_from.module,
                        None,
                        import_from.level,
                        aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                        ImportStyle::From,
                        settings,
                    ),
                    *first_index,
                ),
            })
            .collect::<Vec<_>>();

        let mut eager_imports = vec![];
        let mut lazy_imports = vec![];
        for (import, first_index) in ordered_imports {
            let is_lazy = match &import {
                Import((alias, _)) => alias.is_lazy,
                ImportFrom((import_from, _, _, _)) => import_from.is_lazy,
            };
            if is_lazy {
                lazy_imports.push((import, first_index));
            } else {
                eager_imports.push((import, first_index));
            }
        }

        eager_imports
            .into_iter()
            .chain(lazy_imports)
            .map(|(import, _)| import)
            .collect()
    } else {
        let ordered_straight_imports = straight_imports
            .sorted_by_cached_key(|(alias, comments)| {
                (
                    ModuleKey::from_module(
                        Some(alias.name),
                        alias.asname,
                        0,
                        None,
                        ImportStyle::Straight,
                        settings,
                    ),
                    comments.first_index.unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        let mut eager_straight_imports = vec![];
        let mut lazy_straight_imports = vec![];
        for import in ordered_straight_imports {
            if import.0.is_lazy {
                lazy_straight_imports.push(import);
            } else {
                eager_straight_imports.push(import);
            }
        }

        let ordered_from_imports = from_imports
            .sorted_by_cached_key(|(import_from, first_index, _, _, aliases)| {
                (
                    ModuleKey::from_module(
                        import_from.module,
                        None,
                        import_from.level,
                        aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                        ImportStyle::From,
                        settings,
                    ),
                    *first_index,
                )
            })
            .collect::<Vec<_>>();
        let mut eager_from_imports = vec![];
        let mut lazy_from_imports = vec![];
        for import_from in ordered_from_imports {
            if import_from.0.is_lazy {
                lazy_from_imports.push(import_from);
            } else {
                eager_from_imports.push(import_from);
            }
        }
        if settings.from_first {
            eager_from_imports
                .into_iter()
                .map(|(import_from, _, comments, trailing_comma, aliases)| {
                    ImportFrom((import_from, comments, trailing_comma, aliases))
                })
                .chain(eager_straight_imports.into_iter().map(Import))
                .chain(lazy_from_imports.into_iter().map(
                    |(import_from, _, comments, trailing_comma, aliases)| {
                        ImportFrom((import_from, comments, trailing_comma, aliases))
                    },
                ))
                .chain(lazy_straight_imports.into_iter().map(Import))
                .collect()
        } else {
            eager_straight_imports
                .into_iter()
                .map(Import)
                .chain(eager_from_imports.into_iter().map(
                    |(import_from, _, comments, trailing_comma, aliases)| {
                        ImportFrom((import_from, comments, trailing_comma, aliases))
                    },
                ))
                .chain(lazy_straight_imports.into_iter().map(Import))
                .chain(lazy_from_imports.into_iter().map(
                    |(import_from, _, comments, trailing_comma, aliases)| {
                        ImportFrom((import_from, comments, trailing_comma, aliases))
                    },
                ))
                .collect()
        }
    }
}
