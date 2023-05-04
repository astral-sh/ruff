use itertools::Itertools;
use rustpython_parser::ast::{Alias, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for missing `from __future__ import annotations` import if a type used in the
/// module can be rewritten using PEP 563.
///
/// ## Why is this bad?
/// Pairs well with pyupgrade with the --py37-plus flag or higher, since pyupgrade only
/// replaces type annotations with the PEP 563 rules if `from __future__ import annotations`
/// is present.
///
/// ## Example
/// ```python
/// import typing as t
/// from typing import List
///
/// def function(a_dict: t.Dict[str, t.Optional[int]]) -> None:
///     a_list: List[str] = []
///     a_list.append("hello")
/// ```
///
/// To fix the lint error:
/// ```python
/// from __future__ import annotations
///
/// import typing as t
/// from typing import List
///
/// def function(a_dict: t.Dict[str, t.Optional[int]]) -> None:
///     a_list: List[str] = []
///     a_list.append("hello")
/// ```
///
/// After running additional pyupgrade autofixes:
/// ```python
/// from __future__ import annotations
///
/// def function(a_dict: dict[str, int | None]) -> None:
///     a_list: list[str] = []
///     a_list.append("hello")
/// ```
#[violation]
pub struct MissingFutureAnnotationsWithImports {
    pub names: Vec<String>,
}

impl Violation for MissingFutureAnnotationsWithImports {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MissingFutureAnnotationsWithImports { names } = self;
        let names = names.iter().map(|name| format!("`{name}`")).join(", ");
        format!("Missing `from __future__ import annotations`, but imports: {names}")
    }
}

// PEP_593_SUBSCRIPTS
pub const FUTURE_ANNOTATIONS_REWRITE_ELIGIBLE: &[&[&str]] = &[
    &["typing", "DefaultDict"],
    &["typing", "Deque"],
    &["typing", "Dict"],
    &["typing", "FrozenSet"],
    &["typing", "List"],
    &["typing", "Optional"],
    &["typing", "Set"],
    &["typing", "Tuple"],
    &["typing", "Type"],
    &["typing", "Union"],
    &["typing_extensions", "Type"],
];

/// FA100
pub fn missing_future_annotations_from_typing_import(
    checker: &mut Checker,
    stmt: &Stmt,
    module: &str,
    names: &[Alias],
) {
    if checker.ctx.annotations_future_enabled {
        return;
    }

    let result: Vec<String> = names
        .iter()
        .map(|name| name.node.name.as_str())
        .filter(|alias| FUTURE_ANNOTATIONS_REWRITE_ELIGIBLE.contains(&[module, alias].as_slice()))
        .map(std::string::ToString::to_string)
        .sorted()
        .collect();

    if !result.is_empty() {
        checker.diagnostics.push(Diagnostic::new(
            MissingFutureAnnotationsWithImports { names: result },
            stmt.range(),
        ));
    }
}

/// FA100
pub fn missing_future_annotations_from_typing_usage(checker: &mut Checker, expr: &Expr) {
    if checker.ctx.annotations_future_enabled {
        return;
    }

    if let Some(binding) = checker.ctx.resolve_call_path(expr) {
        if FUTURE_ANNOTATIONS_REWRITE_ELIGIBLE.contains(&binding.as_slice()) {
            checker.diagnostics.push(Diagnostic::new(
                MissingFutureAnnotationsWithImports {
                    names: vec![binding.iter().join(".")],
                },
                expr.range(),
            ));
        }
    }
}
