use log::error;
use rustpython_parser as parser;
use rustpython_parser::ast::{Location, StmtKind, Suite};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::imports::{Alias, AnyImport, FutureImport, Import, ImportFrom};
use ruff_python_ast::source_code::{Locator, Stylist};
use ruff_python_ast::types::Range;

use crate::importer::Importer;
use crate::registry::Rule;
use crate::rules::isort::track::Block;
use crate::settings::{flags, Settings};

/// ## What it does
/// Adds any required imports, as specified by the user, to the top of the
/// file.
///
/// ## Why is this bad?
/// In some projects, certain imports are required to be present in all
/// files. For example, some projects assume that
/// `from __future__ import annotations` is enabled,
/// and thus require that import to be
/// present in all files. Omitting a "required" import (as specified by
/// the user) can cause errors or unexpected behavior.
///
/// ## Example
/// ```python
/// import typing
/// ```
///
/// Use instead:
/// ```python
/// from __future__ import annotations
///
/// import typing
/// ```
#[violation]
pub struct MissingRequiredImport(pub String);

impl AlwaysAutofixableViolation for MissingRequiredImport {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingRequiredImport(name) = self;
        format!("Missing required import: `{name}`")
    }

    fn autofix_title(&self) -> String {
        let MissingRequiredImport(name) = self;
        format!("Insert required import: `{name}`")
    }
}

fn contains(block: &Block, required_import: &AnyImport) -> bool {
    block.imports.iter().any(|import| match required_import {
        AnyImport::Import(required_import) => {
            let StmtKind::Import {
                names,
            } = &import.node else {
                return false;
            };
            names.iter().any(|alias| {
                alias.node.name == required_import.name.name
                    && alias.node.asname.as_deref() == required_import.name.as_name
            })
        }
        AnyImport::ImportFrom(required_import) => {
            let StmtKind::ImportFrom {
                module,
                names,
                level,
            } = &import.node else {
                return false;
            };
            module.as_deref() == required_import.module
                && *level == required_import.level
                && names.iter().any(|alias| {
                    alias.node.name == required_import.name.name
                        && alias.node.asname.as_deref() == required_import.name.as_name
                })
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn add_required_import(
    required_import: &AnyImport,
    blocks: &[&Block],
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    autofix: flags::Autofix,
    is_stub: bool,
) -> Option<Diagnostic> {
    // If the import is already present in a top-level block, don't add it.
    if blocks
        .iter()
        .filter(|block| !block.nested)
        .any(|block| contains(block, required_import))
    {
        return None;
    }

    // Don't add imports to semantically-empty files.
    if python_ast.iter().all(is_docstring_stmt) {
        return None;
    }

    // We don't need to add `__future__` imports to stubs.
    if is_stub && required_import.is_future_import() {
        return None;
    }

    // Always insert the diagnostic at top-of-file.
    let mut diagnostic = Diagnostic::new(
        MissingRequiredImport(required_import.to_string()),
        Range::new(Location::default(), Location::default()),
    );
    if autofix.into() && settings.rules.should_fix(Rule::MissingRequiredImport) {
        diagnostic.set_fix(Importer::new(python_ast, locator, stylist).add_import(required_import));
    }
    Some(diagnostic)
}

/// I002
pub fn add_required_imports(
    blocks: &[&Block],
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    autofix: flags::Autofix,
    is_stub: bool,
) -> Vec<Diagnostic> {
    settings
        .isort
        .required_imports
        .iter()
        .flat_map(|required_import| {
            let Ok(body) = parser::parse_program(required_import, "<filename>") else {
                error!("Failed to parse required import: `{}`", required_import);
                return vec![];
            };
            if body.is_empty() || body.len() > 1 {
                error!(
                    "Expected require import to contain a single statement: `{}`",
                    required_import
                );
                return vec![];
            }
            let stmt = &body[0];
            match &stmt.node {
                StmtKind::ImportFrom {
                    module,
                    names,
                    level,
                } => names
                    .iter()
                    .filter_map(|name| {
                        add_required_import(
                            &AnyImport::ImportFrom(ImportFrom {
                                module: module.as_ref().map(String::as_str),
                                name: Alias {
                                    name: name.node.name.as_str(),
                                    as_name: name.node.asname.as_deref(),
                                },
                                level: *level,
                            }),
                            blocks,
                            python_ast,
                            locator,
                            stylist,
                            settings,
                            autofix,
                            is_stub,
                        )
                    })
                    .collect(),
                StmtKind::Import { names } => names
                    .iter()
                    .filter_map(|name| {
                        add_required_import(
                            &AnyImport::Import(Import {
                                name: Alias {
                                    name: name.node.name.as_str(),
                                    as_name: name.node.asname.as_deref(),
                                },
                            }),
                            blocks,
                            python_ast,
                            locator,
                            stylist,
                            settings,
                            autofix,
                            is_stub,
                        )
                    })
                    .collect(),
                _ => {
                    error!(
                        "Expected required import to be in import-from style: `{}`",
                        required_import
                    );
                    vec![]
                }
            }
        })
        .collect()
}
