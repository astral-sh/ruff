use crate::rules::isort::types::TrailingComma;

use super::types::{AliasData, ImportBlock, ImportFromData};
use super::AnnotatedImport;

pub fn normalize_imports(
    imports: Vec<AnnotatedImport>,
    combine_as_imports: bool,
    force_single_line: bool,
) -> ImportBlock {
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
                trailing_comma,
            } => {
                // Insert comments on the statement itself.
                if let Some(alias) = names.first() {
                    let import_from = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if alias.asname.is_none() || combine_as_imports || force_single_line {
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
                }

                // Create an entry for every alias (import) within the statement.
                for alias in names {
                    let import_from = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if alias.asname.is_none() || combine_as_imports || force_single_line {
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

                    // Propagate trailing commas.
                    if matches!(trailing_comma, TrailingComma::Present) {
                        import_from.trailing_comma = TrailingComma::Present;
                    }
                }
            }
        }
    }
    block
}
