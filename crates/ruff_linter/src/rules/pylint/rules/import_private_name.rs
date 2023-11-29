use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Check for imported symbols that have a leading underscore, known as "private" symbols.
///
/// ## Why is this bad?
/// According to [PEP 8], the underscore prefix is used to indicate that a symbol is private.
/// Private symbols are not meant to be used outside of the module they are defined in.
///
/// [PEP 8]: https://peps.python.org/pep-0008/#descriptive-naming-styles
#[violation]
pub struct ImportPrivateName {
    symbol_type: SymbolType,
}

impl Violation for ImportPrivateName {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ImportPrivateName { symbol_type } = self;
        match symbol_type {
            SymbolType::Module => format!("Imported private module"),
            SymbolType::FromModule => format!("Imported from private module"),
            SymbolType::Object => format!("Imported private object"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymbolType {
    Module,
    FromModule,
    Object,
}

/// C2701
pub(crate) fn import_private_name(checker: &mut Checker, stmt: &Stmt) {
    match stmt {
        Stmt::Import(ast::StmtImport { names, .. }) => {
            for alias in names {
                if alias.name.as_str().starts_with('_') {
                    checker.diagnostics.push(Diagnostic::new(
                        ImportPrivateName {
                            symbol_type: SymbolType::Module,
                        },
                        alias.name.range(),
                    ));
                }
            }
        }
        Stmt::ImportFrom(ast::StmtImportFrom { names, module, .. }) => {
            for alias in names {
                if alias.name.as_str().starts_with('_') {
                    checker.diagnostics.push(Diagnostic::new(
                        ImportPrivateName {
                            symbol_type: SymbolType::Object,
                        },
                        alias.name.range(),
                    ));
                }
            }

            if let Some(identifier) = module {
                if identifier == "__future__" {
                    return;
                }
                if identifier.starts_with('_') {
                    checker.diagnostics.push(Diagnostic::new(
                        ImportPrivateName {
                            symbol_type: SymbolType::Module,
                        },
                        identifier.range(),
                    ));
                } else if identifier.contains("._") {
                    checker.diagnostics.push(Diagnostic::new(
                        ImportPrivateName {
                            symbol_type: SymbolType::FromModule,
                        },
                        identifier.range(),
                    ));
                }
            }
        }
        _ => {}
    }
}
