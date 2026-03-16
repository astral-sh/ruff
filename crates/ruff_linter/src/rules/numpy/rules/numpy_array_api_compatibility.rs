use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::Modules;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{Edit, Fix, FixAvailability, Violation};
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for NumPy's 2.x code compatibility with the [Array API Standard].
///
/// ## Why is this bad?
/// This rule is intended for array consumers who would like to move from
/// NumPy-only code to the Array API compatible one. Array provider vendor
/// lock-in disallows to easily switch between libraries, e.g. from NumPy
/// to JAX or `PyTorch`. Ensuring your code is aligned with the standard makes
/// this procedure less cumbersome and allows to e.g. switch from CPU to
/// GPU backend with a different library.
///
/// This rule is intended for codebases that already use NumPy 2.0 or above
/// as most of the Array API coverage has been shipped in these versions.
///
/// This rule doesn't provide a complete Array API migration, and isn't
/// capable of flagging all standard incompatible API calls, but should
/// flag a large portion of them.
///
/// ## Example
/// ```python
/// import numpy as np
///
/// a, b = np.full((5, 4)), np.asarray(arr)
/// c = np.dot(a, b)
/// res = c.T
/// ```
///
/// Use instead:
/// ```python
/// import numpy as np
///
/// xp = np
///
/// a, b = xp.full((5, 4)), xp.asarray(arr)
/// c = xp.tensordot(a, b)
/// res = c.mT
/// ```
///
/// [Array API Standard]: https://data-apis.org/array-api/latest/API_specification/index.html
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.6")]
pub(crate) struct NumpyArrayAPICompatibility {
    existing: String,
    migration_guide: String,
    code_action: Option<String>,
}

impl Violation for NumpyArrayAPICompatibility {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let NumpyArrayAPICompatibility {
            existing,
            migration_guide,
            code_action: _,
        } = self;
        format!("`{existing}` is not compatible with the Array API. {migration_guide}")
    }

    fn fix_title(&self) -> Option<String> {
        let NumpyArrayAPICompatibility {
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
    AutoImport { name: &'a str },
    Manual { guideline: &'a str },
}

impl Details<'_> {
    fn guideline(&self) -> String {
        match self {
            Details::AutoImport { name } => format!("Use `{name}` instead."),
            Details::Manual { guideline } => ToString::to_string(guideline),
        }
    }

    fn code_action(&self) -> Option<String> {
        match self {
            Details::AutoImport { name } => Some(format!("Replace with `numpy.{name}`")),
            Details::Manual { guideline: _ } => None,
        }
    }
}

/// NPY202
pub(crate) fn numpy_array_api_compatibility(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::NUMPY) {
        return;
    }

    let replacement = match checker.semantic().resolve_qualified_name(expr) {
        Some(qualified_name) => match qualified_name.segments() {
            ["numpy", "transpose"] => Replacement {
                existing: "transpose",
                details: Details::AutoImport {
                    name: "permute_dims",
                },
            },
            ["numpy", "dot"] => Replacement {
                existing: "dot",
                details: Details::Manual {
                    guideline: "Use `np.tensordot(a, b, axes=...)` instead with proper axes.",
                },
            },
            _ => return,
        },
        None => match expr {
            Expr::Attribute(ast::ExprAttribute { value: _, attr, .. }) => match attr.as_str() {
                "T" => Replacement {
                    existing: "T",
                    details: Details::Manual {
                        guideline: "For (stacked) matrix transpose use `.mT` instead. For any other permutation of axes use `np.permute_dims(...)`",
                    },
                },
                _ => return,
            },
            _ => return,
        },
    };

    let mut diagnostic = checker.report_diagnostic(
        NumpyArrayAPICompatibility {
            existing: replacement.existing.to_string(),
            migration_guide: replacement.details.guideline(),
            code_action: replacement.details.code_action(),
        },
        expr.range(),
    );

    match replacement.details {
        Details::AutoImport { name } => {
            diagnostic.try_set_fix(|| {
                let (import_edit, binding) = checker.importer().get_or_import_symbol(
                    &ImportRequest::import_from("numpy", name),
                    expr.start(),
                    checker.semantic(),
                )?;
                let replacement_edit = Edit::range_replacement(binding, expr.range());
                Ok(Fix::safe_edits(import_edit, [replacement_edit]))
            });
        }
        Details::Manual { guideline: _ } => {}
    }
}
