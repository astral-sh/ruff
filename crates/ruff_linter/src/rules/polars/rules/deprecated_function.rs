use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::Expr;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

/// ## What it does
/// Replaces deprecated 0.19.x Polars functions with the newer 1.x versions.
///
/// ## Why is this bad?
/// When Polars functions are deprecated, they are usually replaced with
/// newer, more efficient versions, or with functions that are more
/// consistent with the rest of the Polars API.
///
/// Prefer newer APIs over deprecated ones.
///
/// ## Examples
/// ```python
/// import polars as pl
///
/// pl.avg("a")
/// ```
///
/// Use instead:
/// ```python
/// import polars as pl
///
/// pl.mean("a")
/// ```
#[violation]
pub struct PolarsDeprecatedFunction {
    existing: String,
    replacement: String,
}

impl Violation for PolarsDeprecatedFunction {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PolarsDeprecatedFunction {
            existing,
            replacement,
        } = self;
        format!("`pl.{existing}` is deprecated; use `pl.{replacement}` instead")
    }

    fn fix_title(&self) -> Option<String> {
        let PolarsDeprecatedFunction { replacement, .. } = self;
        Some(format!("Replace with `pl.{replacement}`"))
    }
}

/// POLARS101
pub(crate) fn deprecated_function(checker: &mut Checker, expr: &Expr) {
    if let Some((existing, replacement)) =
        checker
            .semantic()
            .resolve_call_path(expr)
            .and_then(|call_path| match call_path.as_slice() {
                ["polars", "avg"] => Some(("avg", "mean")),
                ["polars", "map"] => Some(("map", "map_batches")),
                ["polars", "apply"] => Some(("apply", "map_groups")),
                ["polars", "cumsum"] => Some(("cumsum", "cum_sum")),
                ["polars", "cumfold"] => Some(("cumfold", "cum_fold")),
                ["polars", "cumreduce"] => Some(("cumreduce", "cum_reduce")),
                ["polars", "cumsum_horizontal"] => {
                    Some(("cumsum_horizontal", "cum_sum_horizontal"))
                }
                _ => None,
            })
    {
        let mut diagnostic = Diagnostic::new(
            PolarsDeprecatedFunction {
                existing: existing.to_string(),
                replacement: replacement.to_string(),
            },
            expr.range(),
        );
        diagnostic.try_set_fix(|| {
            let (import_edit, binding) = checker.importer().get_or_import_symbol(
                &ImportRequest::import_from("polars", replacement),
                expr.start(),
                checker.semantic(),
            )?;
            let replacement_edit = Edit::range_replacement(binding, expr.range());
            Ok(Fix::safe_edits(import_edit, [replacement_edit]))
        });
        checker.diagnostics.push(diagnostic);
    }
}
