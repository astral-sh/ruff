use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    self as ast,
    visitor::{self, Visitor},
};
use ruff_python_semantic::analyze::type_inference::{PythonType, ResolvedPythonType};
use ruff_python_semantic::analyze::typing::is_dict;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks when iterating over keys of a dictionary and extracting the value from the dictionary
/// through indexing the key, instead of calling `.items()` on the dictionary.
///
/// ## Why is this bad?
/// Instead of unnecsarily indexing the the dictionary, it's more semantically clear to extract the value
/// one-per-one with the key, increasing readability.
///
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

#[violation]
pub struct DictIndexMissingItems;

impl Violation for DictIndexMissingItems {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Extracting value from dictionary in iteration without calling `.items()`")
    }
}

struct SubscriptVisitor<'a> {
    target: &'a ast::Expr,
    iter_obj_name: &'a ast::ExprName,
    has_violation: bool,
}

impl<'a> SubscriptVisitor<'a> {
    fn new(target: &'a ast::Expr, iter_obj_name: &'a ast::ExprName) -> Self {
        Self {
            target,
            iter_obj_name,
            has_violation: false,
        }
    }
}

impl<'a> visitor::Visitor<'a> for SubscriptVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a ast::Stmt) {
        // Skip if the assignment is a subscript expression.
        if let ast::Stmt::Assign(assign) = stmt {
            if let Some(target) = assign.targets.first() {
                if target.is_subscript_expr() {
                    return;
                }
            }
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if let ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) = expr {
            if !value.is_name_expr() && !value.is_attribute_expr() {
                return;
            }
            // Check that the sliced value is the same as the target of the for loop.
            let slice_name = slice.as_name_expr();
            let target_name = self.target.as_name_expr();

            if slice_name.is_none() || target_name.is_none() {
                return;
            }

            let slice_name = slice_name.unwrap();
            let target_name = target_name.unwrap();

            if slice_name.id != target_name.id {
                return;
            }

            // Check that the sliced dict name is the same as the iterated object name.
            if !(value
                .as_name_expr()
                .is_some_and(|name| name.id == self.iter_obj_name.id))
            {
                return;
            }

            self.has_violation = true;
        } else {
            visitor::walk_expr(self, expr);
        }
    }
}

/// Extracts the name of the dictionary from the expression.
fn extract_dict_name(expr: &ast::Expr) -> Option<&ast::ExprName> {
    if let Some(name_expr) = expr.as_name_expr() {
        return Some(name_expr);
    }

    // Handle `dict.keys()` case.
    if let ast::Expr::Call(ast::ExprCall { func, .. }) = expr {
        if let ast::Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() {
            if attr == "keys" {
                if let ast::Expr::Name(var_name) = value.as_ref() {
                    return Some(var_name);
                }
            }
        }
    }

    // Handle `my_dict := {"foo": "bar"}` case.
    if let ast::Expr::Named(ast::ExprNamed { target, value, .. }) = expr {
        if let ast::Expr::Dict(ast::ExprDict { .. }) = value.as_ref() {
            if let ast::Expr::Name(var_name) = target.as_ref() {
                return Some(var_name);
            }
        }
    }
    None
}

fn is_inferred_dict(name: &ast::ExprName, checker: &Checker) -> bool {
    let binding = checker
        .semantic()
        .only_binding(name)
        .map(|id| checker.semantic().binding(id));
    if binding.is_none() {
        return false;
    }
    let binding = binding.unwrap();
    is_dict(binding, checker.semantic())
}

/// PLC0206
pub(crate) fn dict_index_missing_items(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    let ast::StmtFor {
        target, iter, body, ..
    } = stmt_for;

    // Check if the right hand side is a dict literal (i.e. `for key in (dict := {"a": 1}):`).
    let is_dict_literal = matches!(
        ResolvedPythonType::from(&**iter),
        ResolvedPythonType::Atom(PythonType::Dict),
    );

    let Some(iter_obj_name) = extract_dict_name(iter) else {
        return;
    };
    if !is_inferred_dict(iter_obj_name, checker) && !is_dict_literal {
        return;
    }

    let mut visitor = SubscriptVisitor::new(target, iter_obj_name);
    for stmt in body {
        visitor.visit_stmt(stmt);
    }

    if visitor.has_violation {
        let diagnostic = Diagnostic::new(DictIndexMissingItems, stmt_for.range);
        checker.diagnostics.push(diagnostic);
    }
}
