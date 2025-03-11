use super::settings::Settings;
use super::types::{AliasData, ImportBlock, ImportFromData, TrailingComma};
use super::AnnotatedImport;

pub(crate) fn normalize_imports<'a>(
    imports: Vec<AnnotatedImport<'a>>,
    settings: &Settings,
) -> ImportBlock<'a> {
    let mut block = ImportBlock::default();
    for import in imports {
        match import {
            AnnotatedImport::Import {
                names,
                atop,
                inline,
            } => {
                // Associate the comments with the first alias (best effort).
                if let Some(name) = names.first() {
                    let comment_set = block
                        .import
                        .entry(AliasData {
                            name: name.name,
                            asname: name.asname,
                        })
                        .or_default();
                    for comment in atop {
                        comment_set.atop.push(comment.value);
                    }
                    for comment in inline {
                        comment_set.inline.push(comment.value);
                    }
                }

                // Create an entry for every alias.
                for name in &names {
                    block
                        .import
                        .entry(AliasData {
                            name: name.name,
                            asname: name.asname,
                        })
                        .or_default();
                }
            }
            AnnotatedImport::ImportFrom {
                module,
                names,
                level,
                atop,
                inline,
                trailing,
                trailing_comma,
            } => {
                // Whether to track each member of the import as a separate entry.
                let isolate_aliases = settings.force_single_line
                    && module
                        .is_none_or(|module| !settings.single_line_exclusions.contains(module))
                    && names.first().is_none_or(|alias| alias.name != "*");

                // Insert comments on the statement itself.
                if isolate_aliases {
                    let mut first = true;
                    for alias in &names {
                        let import_from = block
                            .import_from_as
                            .entry((
                                ImportFromData { module, level },
                                AliasData {
                                    name: alias.name,
                                    asname: alias.asname,
                                },
                            ))
                            .or_default();

                        // Associate the comments above the import statement with the first alias
                        // (best effort).
                        if std::mem::take(&mut first) {
                            for comment in &atop {
                                import_from.comments.atop.push(comment.value.clone());
                            }
                        }

                        // Replicate the inline (and after) comments onto every member.
                        for comment in &inline {
                            import_from.comments.inline.push(comment.value.clone());
                        }
                        for comment in &trailing {
                            import_from.comments.trailing.push(comment.value.clone());
                        }
                    }
                } else {
                    if let Some(alias) = names.first() {
                        let import_from = if alias.name == "*" {
                            block
                                .import_from_star
                                .entry(ImportFromData { module, level })
                                .or_default()
                        } else if alias.asname.is_none() || settings.combine_as_imports {
                            block
                                .import_from
                                .entry(ImportFromData { module, level })
                                .or_default()
                        } else {
                            block
                                .import_from_as
                                .entry((
                                    ImportFromData { module, level },
                                    AliasData {
                                        name: alias.name,
                                        asname: alias.asname,
                                    },
                                ))
                                .or_default()
                        };

                        for comment in atop {
                            import_from.comments.atop.push(comment.value);
                        }
                        for comment in inline {
                            import_from.comments.inline.push(comment.value);
                        }
                        for comment in trailing {
                            import_from.comments.trailing.push(comment.value);
                        }
                    }
                }

                // Create an entry for every alias (member) within the statement.
                for alias in names {
                    let import_from = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if !isolate_aliases
                        && (alias.asname.is_none() || settings.combine_as_imports)
                    {
                        block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else {
                        block
                            .import_from_as
                            .entry((
                                ImportFromData { module, level },
                                AliasData {
                                    name: alias.name,
                                    asname: alias.asname,
                                },
                            ))
                            .or_default()
                    };

                    let comment_set = import_from
                        .aliases
                        .entry(AliasData {
                            name: alias.name,
                            asname: alias.asname,
                        })
                        .or_default();

                    for comment in alias.atop {
                        comment_set.atop.push(comment.value);
                    }
                    for comment in alias.inline {
                        comment_set.inline.push(comment.value);
                    }
                    for comment in alias.trailing {
                        comment_set.trailing.push(comment.value);
                    }

                    // Propagate trailing commas.
                    if !isolate_aliases && matches!(trailing_comma, TrailingComma::Present) {
                        import_from.trailing_comma = TrailingComma::Present;
                    }
                }
            }
        }
    }
    block
}
