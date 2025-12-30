use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::{
    self as ast, Expr, ExprContext,
    visitor::{self, Visitor},
};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing::is_dict;
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::{Checker, DiagnosticGuard};

/// ## What it does
/// Checks for dictionary iterations that extract the dictionary value
/// via explicit indexing, instead of using `.items()`.
///
/// ## Why is this bad?
/// Iterating over a dictionary with `.items()` is semantically clearer
/// and more efficient than extracting the value with the key.
///
/// ## Example
/// ```python
/// ORCHESTRA = {
///     "violin": "strings",
///     "oboe": "woodwind",
///     "tuba": "brass",
///     "gong": "percussion",
/// }
///
/// for instrument in ORCHESTRA:
///     print(f"{instrument}: {ORCHESTRA[instrument]}")
/// ```
///
/// Use instead:
/// ```python
/// ORCHESTRA = {
///     "violin": "strings",
///     "oboe": "woodwind",
///     "tuba": "brass",
///     "gong": "percussion",
/// }
///
/// for instrument, section in ORCHESTRA.items():
///     print(f"{instrument}: {section}")
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "0.8.0")]
pub(crate) struct DictIndexMissingItems;

impl Violation for DictIndexMissingItems {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Extracting value from dictionary without calling `.items()`".to_string()
    }
}

/// PLC0206
pub(crate) fn dict_index_missing_items(checker: &Checker, stmt_for: &ast::StmtFor) {
    let ast::StmtFor {
        target, iter, body, ..
    } = stmt_for;

    // Extract the name of the iteration object (e.g., `obj` in `for key in obj:`).
    let Some(dict_name) = extract_dict_name(iter) else {
        return;
    };

    // Determine if the right-hand side is a dictionary literal (i.e. `for key in (dict := {"a": 1}):`).
    let is_dict_literal = matches!(
        ResolvedPythonType::from(&**iter),
        ResolvedPythonType::Atom(PythonType::Dict),
    );

    if !is_dict_literal && !is_inferred_dict(dict_name, checker.semantic()) {
        return;
    }

    let range = if checker.settings().preview.is_enabled() {
        TextRange::new(target.start(), iter.end())
    } else {
        stmt_for.range()
    };

    SubscriptVisitor::new(target, dict_name, checker, range).visit_body(body);
}

/// A visitor to detect subscript operations on a target dictionary.
struct SubscriptVisitor<'a, 'b> {
    /// The target of the for loop (e.g., `key` in `for key in obj:`).
    target: &'a Expr,
    /// The name of the iterated object (e.g., `obj` in `for key in obj:`).
    dict_name: &'a ast::ExprName,
    range: TextRange,
    checker: &'a Checker<'b>,
    guard: Option<DiagnosticGuard<'a, 'b>>,
}

impl<'a, 'b> SubscriptVisitor<'a, 'b> {
    fn new(
        target: &'a Expr,
        dict_name: &'a ast::ExprName,
        checker: &'a Checker<'b>,
        range: TextRange,
    ) -> Self {
        Self {
            target,
            dict_name,
            range,
            checker,
            guard: None,
        }
    }
}

impl<'a> Visitor<'a> for SubscriptVisitor<'a, '_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        // Given `obj[key]`, `value` must be `obj` and `slice` must be `key`.
        if let Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx: ExprContext::Load,
            ..
        }) = expr
        {
            let Expr::Name(name) = value.as_ref() else {
                return;
            };

            // Check that the sliced dictionary name is the same as the iterated object name.
            if name.id != self.dict_name.id {
                return;
            }

            // Check that the sliced value is the same as the target of the `for` loop.
            if ComparableExpr::from(slice) != ComparableExpr::from(self.target) {
                return;
            }

            if self.guard.is_none() {
                self.guard = Some(
                    self.checker
                        .report_diagnostic(DictIndexMissingItems, self.range),
                );
            }

            if self.checker.settings().preview.is_enabled() {
                self.guard.as_mut().unwrap().secondary_annotation("", expr);
            }
        } else {
            visitor::walk_expr(self, expr);
        }
    }
}

/// Extracts the name of the dictionary from the expression.
fn extract_dict_name(expr: &Expr) -> Option<&ast::ExprName> {
    // Ex) `for key in obj:`
    if let Some(name_expr) = expr.as_name_expr() {
        return Some(name_expr);
    }

    // Ex) `for key in obj.keys():`
    if let Expr::Call(ast::ExprCall { func, .. }) = expr {
        if let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() {
            if attr == "keys" {
                if let Expr::Name(var_name) = value.as_ref() {
                    return Some(var_name);
                }
            }
        }
    }

    // Ex) `for key in (my_dict := {"foo": "bar"}):`
    if let Expr::Named(ast::ExprNamed { target, value, .. }) = expr {
        if let Expr::Dict(ast::ExprDict { .. }) = value.as_ref() {
            if let Expr::Name(var_name) = target.as_ref() {
                return Some(var_name);
            }
        }
    }

    None
}

/// Returns `true` if the binding is a dictionary, inferred from the type.
fn is_inferred_dict(name: &ast::ExprName, semantic: &SemanticModel) -> bool {
    semantic
        .only_binding(name)
        .map(|id| semantic.binding(id))
        .is_some_and(|binding| is_dict(binding, semantic))
}
