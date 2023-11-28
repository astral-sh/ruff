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
/// It is more succinct to use the variable for the value at the current index which is already in scope from the iterator. 
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
        format!("Unnecessary lookup of list item by index")
    }

    fn fix_title(&self) -> String {
        format!("Use existing item variable instead")
    }
}

struct SubscriptVisitor<'a> {
    sequence_name: &'a str,
    index_name: &'a str,
    diagnostic_ranges: Vec<TextRange>,
    is_subcript_modified: bool,
}

impl<'a> SubscriptVisitor<'a> {
    fn new(sequence_name: &'a str, index_name: &'a str) -> Self {
        Self {
            sequence_name,
            index_name,
            diagnostic_ranges: Vec::new(),
            is_subcript_modified: false,
        }
    }
}

fn check_target_for_assignment(expr: &Expr, sequence_name: &str, index_name: &str) -> bool {
    // if we see the sequence subscript being modified, we'll stop emitting diagnostics
    match expr {
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                if id == sequence_name {
                    if let Expr::Name(ast::ExprName { id, .. }) = slice.as_ref() {
                        if id == index_name {
                            return true;
                        }
                    }
                }
            }
            false
        }
        _ => false,
    }
}

impl<'a> Visitor<'_> for SubscriptVisitor<'a> {
    fn visit_expr(&mut self, expr: &Expr) {
        if self.is_subcript_modified {
            return;
        }
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
        if self.is_subcript_modified {
            return;
        }
        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                self.is_subcript_modified = targets.iter().any(|target| {
                    check_target_for_assignment(target, self.sequence_name, self.index_name)
                });
                self.visit_expr(value);
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                if let Some(value) = value {
                    self.is_subcript_modified =
                        check_target_for_assignment(target, self.sequence_name, self.index_name);
                    self.visit_expr(value);
                }
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                self.is_subcript_modified =
                    check_target_for_assignment(target, self.sequence_name, self.index_name);
                self.visit_expr(value);
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// PLR1736
pub(crate) fn unnecessary_list_index_lookup(checker: &mut Checker, stmt_for: &StmtFor) {
    let Some((sequence, index_name, value_name)) =
        enumerate_items(checker, &stmt_for.iter, &stmt_for.target)
    else {
        return;
    };

    let mut visitor = SubscriptVisitor::new(&sequence, &index_name);

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

/// PLR1736
pub(crate) fn unnecessary_list_index_lookup_comprehension(checker: &mut Checker, expr: &Expr) {
    match expr {
        Expr::GeneratorExp(ast::ExprGeneratorExp {
            elt, generators, ..
        })
        | Expr::DictComp(ast::ExprDictComp {
            value: elt,
            generators,
            ..
        })
        | Expr::SetComp(ast::ExprSetComp {
            elt, generators, ..
        })
        | Expr::ListComp(ast::ExprListComp {
            elt, generators, ..
        }) => {
            for comp in generators {
                if let Some((sequence, index_name, value_name)) =
                    enumerate_items(checker, &comp.iter, &comp.target)
                {
                    let mut visitor = SubscriptVisitor::new(&sequence, &index_name);

                    visitor.visit_expr(elt.as_ref());

                    for range in visitor.diagnostic_ranges {
                        let mut diagnostic = Diagnostic::new(UnnecessaryListIndexLookup, range);

                        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                            value_name.clone(),
                            range,
                        )));

                        checker.diagnostics.push(diagnostic);
                    }
                }
            }
        }
        _ => (),
    }
}

fn enumerate_items(
    checker: &mut Checker,
    call_expr: &Expr,
    tuple_expr: &Expr,
) -> Option<(String, String, String)> {
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = call_expr
    else {
        return None;
    };

    // Check that the function is the `enumerate` builtin.
    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return None;
    };
    if id != "enumerate" {
        return None;
    };
    if !checker.semantic().is_builtin("enumerate") {
        return None;
    };

    let Expr::Tuple(ast::ExprTuple { elts, .. }) = tuple_expr else {
        return None;
    };
    let [index, value] = elts.as_slice() else {
        return None;
    };

    // Grab the variable names
    let Expr::Name(ast::ExprName { id: index_name, .. }) = index else {
        return None;
    };

    let Expr::Name(ast::ExprName { id: value_name, .. }) = value else {
        return None;
    };

    // If either of the variable names are intentionally ignored by naming them `_`, then don't emit
    if index_name == "_" || value_name == "_" {
        return None;
    }

    // Get the first argument of the enumerate call
    let Some(Expr::Name(ast::ExprName { id: sequence, .. })) = arguments.args.first() else {
        return None;
    };

    Some((sequence.clone(), index_name.clone(), value_name.clone()))
}
