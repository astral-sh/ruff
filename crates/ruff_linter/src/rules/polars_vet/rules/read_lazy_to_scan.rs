use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for eager Polars reads that are immediately converted to lazy mode.
///
/// ## Why is this bad?
/// Chaining `read_*().lazy()` performs an eager read first, then converts the
/// in-memory result to a lazy plan. When available, `scan_*()` creates the lazy
/// plan directly and is the idiomatic Polars API.
///
/// ## Example
/// ```python
/// import polars as pl
///
/// df = pl.read_csv("data.csv").lazy()
/// ```
///
/// Use instead:
/// ```python
/// import polars as pl
///
/// df = pl.scan_csv("data.csv")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0")]
pub(crate) struct PolarsReadLazyToScan {
    scan_method: String,
}

impl Violation for PolarsReadLazyToScan {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let PolarsReadLazyToScan { scan_method } = self;
        format!("Replace `read_*().lazy()` with `{scan_method}()`")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with a `scan_*()` call".to_string())
    }
}

/// POL001
pub(crate) fn read_lazy_to_scan(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::POLARS) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = call.func.as_ref() else {
        return;
    };

    if attr != "lazy" {
        return;
    }

    let Expr::Call(inner_call) = value.as_ref() else {
        return;
    };

    let Some(read_method) = checker
        .semantic()
        .resolve_qualified_name(&inner_call.func)
        .and_then(|qualified_name| {
            if matches!(
                qualified_name.segments(),
                [
                    "polars",
                    "read_csv" | "read_parquet" | "read_ndjson" | "read_ipc"
                ]
            ) {
                Some(qualified_name.segments()[1])
            } else {
                None
            }
        })
    else {
        return;
    };

    let scan_method = match read_method {
        "read_csv" => "scan_csv",
        "read_parquet" => "scan_parquet",
        "read_ndjson" => "scan_ndjson",
        "read_ipc" => "scan_ipc",
        _ => return,
    };

    let Some(read_method_range) = inner_call
        .func
        .as_attribute_expr()
        .map(|attribute| attribute.attr.range())
    else {
        return;
    };

    let mut diagnostic = checker.report_diagnostic(
        PolarsReadLazyToScan {
            scan_method: scan_method.to_string(),
        },
        call.range(),
    );

    if call.arguments.args.is_empty() && call.arguments.keywords.is_empty() {
        diagnostic.set_fix(Fix::unsafe_edits(
            Edit::range_replacement(scan_method.to_string(), read_method_range),
            [Edit::range_deletion(TextRange::new(
                inner_call.end(),
                call.end(),
            ))],
        ));
    }
}
