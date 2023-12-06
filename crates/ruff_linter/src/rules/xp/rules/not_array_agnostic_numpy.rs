/// use ...
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
///

#[violation]
pub struct NotArrayAgnosticNumPy {
    existing: String,
    migration_guide: Option<String>,
    code_action: Option<String>,
}

impl Violation for NotArrayAgnosticNumPy {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NotArrayAgnosticNumPy {
            existing,
            migration_guide,
            code_action: _,
        } = self;
        match migration_guide {
            Some(migration_guide) => {
                format!("`{existing}` is not in the array API standard. {migration_guide}",)
            }
            None => format!("`{existing}` is not in the array API standard."),
        }
    }

    fn fix_title(&self) -> Option<String> {
        let NotArrayAgnosticNumPy {
            existing: _,
            migration_guide: _,
            code_action,
        } = self;
        code_action.clone()
    }
}

#[derive(Debug)]
struct Replacement<'a> {
    existing: &'a str,
    details: Details<'a>,
}

#[derive(Debug)]
enum Details<'a> {
    /// There is a direct replacement in the array API standard.
    AutoImport { path: &'a str, name: &'a str },
    /// There is no direct replacement in the standard.
    Manual { guideline: Option<&'a str> },
}

impl Details<'_> {
    fn guideline(&self) -> Option<String> {
        match self {
            Details::AutoImport { path, name } => Some(format!("Use `{path}.{name}` instead.")),
            Details::Manual { guideline } => guideline.map(ToString::to_string),
        }
    }

    fn code_action(&self) -> Option<String> {
        match self {
            Details::AutoImport { path, name } => Some(format!("Replace with `{path}.{name}`")),
            Details::Manual { guideline: _ } => None,
        }
    }
}

///XP001
pub(crate) fn not_array_agnostic_numpy(checker: &mut Checker, expr: &Expr) {
    let maybe_replacement = checker
        .semantic()
        .resolve_call_path(expr)
        .and_then(|call_path| match call_path.as_slice() {
            ["numpy", "arccos"] => Some(Replacement {
                existing: "arccos",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "acos",
                },
            }),
            ["numpy", "arccosh"] => Some(Replacement {
                existing: "arccosh",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "acosh",
                },
            }),
            ["numpy", "arcsin"] => Some(Replacement {
                existing: "arcsin",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "asin",
                },
            }),
            ["numpy", "arcsinh"] => Some(Replacement {
                existing: "arcsinh",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "asinh",
                },
            }),
            ["numpy", "arctan"] => Some(Replacement {
                existing: "arctan",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "atan",
                },
            }),
            ["numpy", "arctan2"] => Some(Replacement {
                existing: "arctan2",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "atan2",
                },
            }),
            ["numpy", "arctanh"] => Some(Replacement {
                existing: "arctanh",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "atanh",
                },
            }),
            ["numpy", "left_shift"] => Some(Replacement {
                existing: "left_shift",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bitwise_left_shift",
                },
            }),
            ["numpy", "arccos"] => Some(Replacement {
                existing: "arccos",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "acos",
                },
            }),
            ["numpy", "invert"] => Some(Replacement {
                existing: "invert",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bitwise_invert",
                },
            }),
            ["numpy", "right_shift"] => Some(Replacement {
                existing: "right_shift",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bitwise_right_shift",
                },
            }),
            ["numpy", "bool_"] => Some(Replacement {
                existing: "bool_",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "bool",
                },
            }),
            ["numpy", "concatenate"] => Some(Replacement {
                existing: "concatenate",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "concat",
                },
            }),
            ["numpy", "power"] => Some(Replacement {
                existing: "power",
                details: Details::AutoImport {
                    path: "numpy",
                    name: "pow",
                },
            }),
            _ => None,
        });

    if let Some(replacement) = maybe_replacement {
        let mut diagnostic = Diagnostic::new(
            NotArrayAgnosticNumPy {
                existing: replacement.existing.to_string(),
                migration_guide: replacement.details.guideline(),
                code_action: replacement.details.code_action(),
            },
            expr.range(),
        );
        match replacement.details {
            Details::AutoImport { path, name } => {
                diagnostic.try_set_fix(|| {
                    let (import_edit, binding) = checker.importer().get_or_import_symbol(
                        &ImportRequest::import_from(path, name),
                        expr.start(),
                        checker.semantic(),
                    )?;
                    let replacement_edit = Edit::range_replacement(binding, expr.range());
                    Ok(Fix::unsafe_edits(import_edit, [replacement_edit]))
                });
            }
            Details::Manual { guideline: _ } => {}
        };
        checker.diagnostics.push(diagnostic);
    }
}
