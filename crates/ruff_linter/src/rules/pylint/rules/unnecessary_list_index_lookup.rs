use ruff_python_ast::{self as ast, Expr, Stmt, StmtFor};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of enumeration and accessing the value by index lookup.
///
/// ## Why is this bad?
/// The value is already accessible by the 2nd variable from the enumeration.
///
/// ## Example
/// ```python
/// letters = ["a", "b", "c"]
///
/// for index, letter in enumerate(letters):
///     print(letters[index])
/// ```
///
/// Use instead:
/// ```python
/// letters = ["a", "b", "c"]
///
/// for index, letter in enumerate(letters):
///     print(letter)
/// ```
#[violation]
pub struct UnnecessaryListIndexLookup;

impl AlwaysFixableViolation for UnnecessaryListIndexLookup {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary list index lookup")
    }

    fn fix_title(&self) -> String {
        format!("Remove unnecessary list index lookup")
    }
}

struct SubscriptVisitor<'a> {
    sequence_name: &'a str,
    index_name: &'a str,
    diagnostic_ranges: Vec<TextRange>,
}

impl<'a> SubscriptVisitor<'a> {
    fn new(sequence_name: &'a str, index_name: &'a str) -> Self {
        Self {
            sequence_name,
            index_name,
            diagnostic_ranges: Vec::new(),
        }
    }
}

impl<'a> Visitor<'_> for SubscriptVisitor<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Subscript(ast::ExprSubscript {
                value,
                slice,
                range,
                ..
            }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                    if id == self.sequence_name {
                        if let Expr::Name(ast::ExprName { id, .. }) = slice.as_ref() {
                            if id == self.index_name {
                                self.diagnostic_ranges.push(*range);
                            }
                        }
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Assign(ast::StmtAssign { value, .. }) => {
                self.visit_expr(value);
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { value, .. }) => {
                if let Some(value) = value {
                    self.visit_expr(value);
                }
            }
            Stmt::AugAssign(ast::StmtAugAssign { value, .. }) => {
                self.visit_expr(value);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// PLR1736
pub(crate) fn unnecessary_list_index_lookup(checker: &mut Checker, stmt_for: &StmtFor) {
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = stmt_for.iter.as_ref()
    else {
        return;
    };

    // Check that the function is the `enumerate` builtin.
    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return;
    };
    if id != "enumerate" {
        return;
    };
    if !checker.semantic().is_builtin("enumerate") {
        return;
    };

    let Expr::Tuple(ast::ExprTuple { elts, .. }) = stmt_for.target.as_ref() else {
        return;
    };
    let [index, value] = elts.as_slice() else {
        return;
    };

    // Grab the variable names
    let Expr::Name(ast::ExprName { id: index_name, .. }) = index else {
        return;
    };

    let Expr::Name(ast::ExprName { id: value_name, .. }) = value else {
        return;
    };

    // If either of the variable names are intentionally ignored by naming them `_`, then don't emit
    if index_name == "_" || value_name == "_" {
        return;
    }

    // Get the first argument of the enumerate call
    let Some(Expr::Name(ast::ExprName { id: sequence, .. })) = arguments.args.first() else {
        return;
    };

    let mut visitor = SubscriptVisitor::new(sequence, index_name);

    visitor.visit_body(&stmt_for.body);
    visitor.visit_body(&stmt_for.orelse);

    for range in visitor.diagnostic_ranges {
        let mut diagnostic = Diagnostic::new(UnnecessaryListIndexLookup, range);

        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            value_name.clone(),
            range,
        )));

        checker.diagnostics.push(diagnostic);
    }
}
