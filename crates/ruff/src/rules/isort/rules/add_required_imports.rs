use log::error;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::imports::{Alias, AnyImport, FutureImport, Import, ImportFrom};
use ruff_python_ast::{self as ast, PySourceType, Stmt, Suite};
use ruff_python_codegen::Stylist;
use ruff_python_parser::parse_suite;
use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};

use crate::importer::Importer;
use crate::registry::Rule;
use crate::settings::Settings;

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

/// Return `true` if the [`Stmt`] includes the given [`AnyImport`].
fn includes_import(stmt: &Stmt, target: &AnyImport) -> bool {
    match target {
        AnyImport::Import(target) => {
            let Stmt::Import(ast::StmtImport { names, range: _ }) = &stmt else {
                return false;
            };
            names.iter().any(|alias| {
                &alias.name == target.name.name && alias.asname.as_deref() == target.name.as_name
            })
        }
        AnyImport::ImportFrom(target) => {
            let Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range: _,
            }) = &stmt
            else {
                return false;
            };
            module.as_deref() == target.module
                && level.map(|level| level.to_u32()) == target.level
                && names.iter().any(|alias| {
                    &alias.name == target.name.name
                        && alias.asname.as_deref() == target.name.as_name
                })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn add_required_import(
    required_import: &AnyImport,
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    source_type: PySourceType,
) -> Option<Diagnostic> {
    // Don't add imports to semantically-empty files.
    if python_ast.iter().all(is_docstring_stmt) {
        return None;
    }

    // We don't need to add `__future__` imports to stubs.
    if source_type.is_stub() && required_import.is_future_import() {
        return None;
    }

    // If the import is already present in a top-level block, don't add it.
    if python_ast
        .iter()
        .any(|stmt| includes_import(stmt, required_import))
    {
        return None;
    }

    // Always insert the diagnostic at top-of-file.
    let mut diagnostic = Diagnostic::new(
        MissingRequiredImport(required_import.to_string()),
        TextRange::default(),
    );
    if settings.rules.should_fix(Rule::MissingRequiredImport) {
        diagnostic.set_fix(Fix::automatic(
            Importer::new(python_ast, locator, stylist)
                .add_import(required_import, TextSize::default()),
        ));
    }
    Some(diagnostic)
}

/// I002
pub(crate) fn add_required_imports(
    python_ast: &Suite,
    locator: &Locator,
    stylist: &Stylist,
    settings: &Settings,
    source_type: PySourceType,
) -> Vec<Diagnostic> {
    settings
        .isort
        .required_imports
        .iter()
        .flat_map(|required_import| {
            let Ok(body) = parse_suite(required_import, "<filename>") else {
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
            match stmt {
                Stmt::ImportFrom(ast::StmtImportFrom {
                    module,
                    names,
                    level,
                    range: _,
                }) => names
                    .iter()
                    .filter_map(|name| {
                        add_required_import(
                            &AnyImport::ImportFrom(ImportFrom {
                                module: module.as_deref(),
                                name: Alias {
                                    name: name.name.as_str(),
                                    as_name: name.asname.as_deref(),
                                },
                                level: level.map(|level| level.to_u32()),
                            }),
                            python_ast,
                            locator,
                            stylist,
                            settings,
                            source_type,
                        )
                    })
                    .collect(),
                Stmt::Import(ast::StmtImport { names, range: _ }) => names
                    .iter()
                    .filter_map(|name| {
                        add_required_import(
                            &AnyImport::Import(Import {
                                name: Alias {
                                    name: name.name.as_str(),
                                    as_name: name.asname.as_deref(),
                                },
                            }),
                            python_ast,
                            locator,
                            stylist,
                            settings,
                            source_type,
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
