use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    visitor::{self, Visitor},
    Expr, ExprAttribute, ExprCall, ExprName, ExprSubscript, Stmt, StmtDelete, StmtFor,
};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;
use ruff_diagnostics::Diagnostic;

fn is_mutating_function(function_name: &str) -> bool {
    matches!(
        function_name,
        "append"
            | "sort"
            | "reverse"
            | "remove"
            | "clear"
            | "extend"
            | "insert"
            | "pop"
            | "popitem"
    )
}
/// ## What it does
/// Checks for mutation of the iterator of a loop in the loop's body
///
/// ## Why is this bad?
/// Changing the structure that is being iterated over will usually lead to
/// unintended behavior as not all elements will be addressed.
///
/// ## Example
/// ```python
/// some_list = [1,2,3]
/// for i in some_list:
///   some_list.remove(i) # this will lead to not all elements being printed
///   print(i)
/// ```
///
///
/// ## References
/// - [Python documentation: Mutable Sequence Types](https://docs.python.org/3/library/stdtypes.html#typesseq-mutable)
#[violation]
pub struct LoopIteratorMutation;

impl Violation for LoopIteratorMutation {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("editing a loop's mutable iterable often leads to unexpected results/bugs")
    }
}

fn _to_name_str(node: &Expr) -> String {
    match node {
        Expr::Name(ExprName { id, .. }) => {
            return id.to_string();
        }
        Expr::Attribute(ExprAttribute {
            range: _,
            value,
            attr,
            ..
        }) => {
            let mut inner = _to_name_str(value);
            match inner.as_str() {
                "" => {
                    return "".into();
                }
                _ => {
                    inner.push_str(".");
                    inner.push_str(attr);
                    return inner;
                }
            }
        }
        Expr::Call(ExprCall { range: _, func, .. }) => {
            return _to_name_str(func);
        }
        _ => {
            return "".into();
        }
    }
}
// B909
pub(crate) fn loop_iterator_mutation(checker: &mut Checker, stmt_for: &StmtFor) {
    let StmtFor {
        target: _,
        iter,
        body,
        orelse: _,
        is_async: _,
        range: _,
    } = stmt_for;
    let name;

    match iter.as_ref() {
        Expr::Name(ExprName { .. }) => {
            name = _to_name_str(iter.as_ref());
        }
        Expr::Attribute(ExprAttribute { .. }) => {
            name = _to_name_str(iter.as_ref());
        }
        _ => {
            println!("Shouldn't happen");
            return;
        }
    }
    let mut visitor = LoopMutationsVisitor {
        name: &name,
        mutations: Vec::new(),
    };
    visitor.visit_body(body);
    for mutation in visitor.mutations {
        checker
            .diagnostics
            .push(Diagnostic::new(LoopIteratorMutation, mutation));
    }
}
struct LoopMutationsVisitor<'a> {
    name: &'a str,
    mutations: Vec<TextRange>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoopMutationsVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Delete(StmtDelete { range, targets }) => {
                for target in targets {
                    let name;
                    match target {
                        Expr::Subscript(ExprSubscript {
                            range: _,
                            value,
                            slice: _,
                            ctx: _,
                        }) => {
                            name = _to_name_str(value);
                        }

                        Expr::Attribute(_) | Expr::Name(_) => {
                            name = _to_name_str(target);
                        }
                        _ => {
                            name = String::new();
                            visitor::walk_expr(self, target);
                        }
                    }
                    if self.name.eq(&name) {
                        self.mutations.push(*range);
                    }
                }
            }
            _ => {
                visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(ExprCall {
                range: _,
                func,
                arguments: _,
            }) => match func.as_ref() {
                Expr::Attribute(ExprAttribute {
                    range,
                    value,
                    attr,
                    ctx: _,
                }) => {
                    let name = _to_name_str(value);
                    if self.name.eq(&name) && is_mutating_function(&attr.as_str()) {
                        self.mutations.push(*range);
                    }
                }
                _ => {}
            },
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}
