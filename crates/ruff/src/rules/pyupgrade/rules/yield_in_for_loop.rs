use rustc_hash::FxHashMap;
use rustpython_parser::ast::{self, Expr, ExprContext, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_ast::types::RefEquality;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{statement_visitor, visitor};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for `for` loops that can be replaced with `yield from` expressions.
///
/// ## Why is this bad?
/// If a `for` loop only contains a `yield` statement, it can be replaced with a
/// `yield from` expression, which is more concise and idiomatic.
///
/// ## Example
/// ```python
/// for x in foo:
///     yield x
/// ```
///
/// Use instead:
/// ```python
/// yield from foo
/// ```
///
/// ## References
/// - [Python documentation: The `yield` statement](https://docs.python.org/3/reference/simple_stmts.html#the-yield-statement)
/// - [PEP 380](https://peps.python.org/pep-0380/)
#[violation]
pub struct YieldInForLoop;

impl AlwaysAutofixableViolation for YieldInForLoop {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace `yield` over `for` loop with `yield from`")
    }

    fn autofix_title(&self) -> String {
        "Replace with `yield from`".to_string()
    }
}

/// Return `true` if the two expressions are equivalent, and consistent solely
/// of tuples and names.
fn is_same_expr(a: &Expr, b: &Expr) -> bool {
    match (&a, &b) {
        (Expr::Name(ast::ExprName { id: a, .. }), Expr::Name(ast::ExprName { id: b, .. })) => {
            a == b
        }
        (
            Expr::Tuple(ast::ExprTuple { elts: a, .. }),
            Expr::Tuple(ast::ExprTuple { elts: b, .. }),
        ) => a.len() == b.len() && a.iter().zip(b).all(|(a, b)| is_same_expr(a, b)),
        _ => false,
    }
}

/// Collect all named variables in an expression consisting solely of tuples and
/// names.
fn collect_names(expr: &Expr) -> Vec<&str> {
    match expr {
        Expr::Name(ast::ExprName { id, .. }) => vec![id],
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().flat_map(collect_names).collect(),
        _ => panic!("Expected: Expr::Name | Expr::Tuple"),
    }
}

#[derive(Debug)]
struct YieldFrom<'a> {
    stmt: &'a Stmt,
    body: &'a Stmt,
    iter: &'a Expr,
    names: Vec<&'a str>,
}

#[derive(Default)]
struct YieldFromVisitor<'a> {
    yields: Vec<YieldFrom<'a>>,
}

impl<'a> StatementVisitor<'a> for YieldFromVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::For(ast::StmtFor {
                target,
                body,
                orelse,
                iter,
                ..
            }) => {
                // If there is an else statement, don't rewrite.
                if !orelse.is_empty() {
                    return;
                }
                // If there's any logic besides a yield, don't rewrite.
                if body.len() != 1 {
                    return;
                }
                // If the body is not a yield, don't rewrite.
                let body = &body[0];
                if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = &body {
                    if let Expr::Yield(ast::ExprYield {
                        value: Some(value),
                        range: _,
                    }) = value.as_ref()
                    {
                        if is_same_expr(target, value) {
                            self.yields.push(YieldFrom {
                                stmt,
                                body,
                                iter,
                                names: collect_names(target),
                            });
                        }
                    }
                }
            }
            Stmt::FunctionDef(_) | Stmt::AsyncFunctionDef(_) | Stmt::ClassDef(_) => {
                // Don't recurse into anything that defines a new scope.
            }
            _ => statement_visitor::walk_stmt(self, stmt),
        }
    }
}

#[derive(Default)]
struct ReferenceVisitor<'a> {
    parent: Option<&'a Stmt>,
    references: FxHashMap<RefEquality<'a, Stmt>, Vec<&'a str>>,
}

impl<'a> Visitor<'a> for ReferenceVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        let prev_parent = self.parent;
        self.parent = Some(stmt);
        visitor::walk_stmt(self, stmt);
        self.parent = prev_parent;
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, ctx, range: _ }) => {
                if matches!(ctx, ExprContext::Load | ExprContext::Del) {
                    if let Some(parent) = self.parent {
                        self.references
                            .entry(RefEquality(parent))
                            .or_default()
                            .push(id);
                    }
                }
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// UP028
pub(crate) fn yield_in_for_loop(checker: &mut Checker, stmt: &Stmt) {
    // Intentionally omit async functions.
    if let Stmt::FunctionDef(ast::StmtFunctionDef { body, .. }) = stmt {
        let yields = {
            let mut visitor = YieldFromVisitor::default();
            visitor.visit_body(body);
            visitor.yields
        };

        let references = {
            let mut visitor = ReferenceVisitor::default();
            visitor.visit_body(body);
            visitor.references
        };

        for item in yields {
            // If any of the bound names are used outside of the loop, don't rewrite.
            if references.iter().any(|(stmt, names)| {
                stmt != &RefEquality(item.stmt)
                    && stmt != &RefEquality(item.body)
                    && item.names.iter().any(|name| names.contains(name))
            }) {
                continue;
            }

            let mut diagnostic = Diagnostic::new(YieldInForLoop, item.stmt.range());
            if checker.patch(diagnostic.kind.rule()) {
                let contents = checker.locator.slice(item.iter.range());
                let contents = format!("yield from {contents}");
                diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
                    contents,
                    item.stmt.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
