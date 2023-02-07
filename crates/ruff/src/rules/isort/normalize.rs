use super::types::{AliasData, ImportBlock, ImportFromData, TrailingComma};
use super::AnnotatedImport;

pub fn normalize_imports(imports: Vec<AnnotatedImport>, combine_as_imports: bool) -> ImportBlock {
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
                    let entry = block
                        .import
                        .entry(AliasData {
                            name: name.name,
                            asname: name.asname,
                        })
                        .or_default();
                    for comment in atop {
                        entry.atop.push(comment.value);
                    }
                    for comment in inline {
                        entry.inline.push(comment.value);
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
                if let Some(alias) = names.first() {
                    let entry = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if alias.asname.is_none() || combine_as_imports {
                        &mut block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                            .0
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
                        entry.atop.push(comment.value);
                    }

                    for comment in inline {
                        entry.inline.push(comment.value);
                    }
                }

                // Create an entry for every alias.
                for alias in names {
                    let entry = if alias.name == "*" {
                        block
                            .import_from_star
                            .entry(ImportFromData { module, level })
                            .or_default()
                    } else if alias.asname.is_none() || combine_as_imports {
                        block
                            .import_from
                            .entry(ImportFromData { module, level })
                            .or_default()
                            .1
                            .entry(AliasData {
                                name: alias.name,
                                asname: alias.asname,
                            })
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

                    for comment in alias.atop {
                        entry.atop.push(comment.value);
                    }
                    for comment in alias.inline {
                        entry.inline.push(comment.value);
                    }
                }

                // Propagate trailing commas.
                if matches!(trailing_comma, TrailingComma::Present) {
                    if let Some(entry) =
                        block.import_from.get_mut(&ImportFromData { module, level })
                    {
                        entry.2 = trailing_comma;
                    }
                }
            }
        }
    }
    block
}
