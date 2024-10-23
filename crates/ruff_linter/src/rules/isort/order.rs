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
                            .collect::<Vec<(AliasData, ImportFromCommentSet)>>(),
                    )
                },
            );

    let ordered_imports = if matches!(section, ImportSection::Known(ImportType::Future)) {
        from_imports
            .sorted_by_cached_key(|(import_from, _, _, aliases)| {
                ModuleKey::from_module(
                    import_from.module,
                    None,
                    import_from.level,
                    aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                    ImportStyle::From,
                    settings,
                )
            })
            .map(ImportFrom)
            .chain(
                straight_imports
                    .sorted_by_cached_key(|(alias, _)| {
                        ModuleKey::from_module(
                            Some(alias.name),
                            alias.asname,
                            0,
                            None,
                            ImportStyle::Straight,
                            settings,
                        )
                    })
                    .map(Import),
            )
            .collect()
    } else if settings.force_sort_within_sections {
        straight_imports
            .map(Import)
            .chain(from_imports.map(ImportFrom))
            .sorted_by_cached_key(|import| match import {
                Import((alias, _)) => ModuleKey::from_module(
                    Some(alias.name),
                    alias.asname,
                    0,
                    None,
                    ImportStyle::Straight,
                    settings,
                ),
                ImportFrom((import_from, _, _, aliases)) => ModuleKey::from_module(
                    import_from.module,
                    None,
                    import_from.level,
                    aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                    ImportStyle::From,
                    settings,
                ),
            })
            .collect()
    } else {
        let ordered_straight_imports = straight_imports.sorted_by_cached_key(|(alias, _)| {
            ModuleKey::from_module(
                Some(alias.name),
                alias.asname,
                0,
                None,
                ImportStyle::Straight,
                settings,
            )
        });
        let ordered_from_imports =
            from_imports.sorted_by_cached_key(|(import_from, _, _, aliases)| {
                ModuleKey::from_module(
                    import_from.module,
                    None,
                    import_from.level,
                    aliases.first().map(|(alias, _)| (alias.name, alias.asname)),
                    ImportStyle::From,
                    settings,
                )
            });
        if settings.from_first {
            ordered_from_imports
                .into_iter()
                .map(ImportFrom)
                .chain(ordered_straight_imports.into_iter().map(Import))
                .collect()
        } else {
            ordered_straight_imports
                .into_iter()
                .map(Import)
                .chain(ordered_from_imports.into_iter().map(ImportFrom))
                .collect()
        }
    };

    ordered_imports
}
