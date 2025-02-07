use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::types::Node;
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Comprehension, Expr, ExprContext, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function definitions that use a loop variable.
///
/// ## Why is this bad?
/// The loop variable is not bound in the function definition, so it will always
/// have the value it had in the last iteration when the function is called.
///
/// Instead, consider using a default argument to bind the loop variable at
/// function definition time. Or, use `functools.partial`.
///
/// ## Example
/// ```python
/// adders = [lambda x: x + i for i in range(3)]
/// values = [adder(1) for adder in adders]  # [3, 3, 3]
/// ```
///
/// Use instead:
/// ```python
/// adders = [lambda x, i=i: x + i for i in range(3)]
/// values = [adder(1) for adder in adders]  # [1, 2, 3]
/// ```
///
/// Or:
/// ```python
/// from functools import partial
///
/// adders = [partial(lambda x, i: x + i, i=i) for i in range(3)]
/// values = [adder(1) for adder in adders]  # [1, 2, 3]
/// ```
///
/// ## References
/// - [The Hitchhiker's Guide to Python: Late Binding Closures](https://docs.python-guide.org/writing/gotchas/#late-binding-closures)
/// - [Python documentation: `functools.partial`](https://docs.python.org/3/library/functools.html#functools.partial)
#[derive(ViolationMetadata)]
pub(crate) struct FunctionUsesLoopVariable {
    name: String,
}

impl Violation for FunctionUsesLoopVariable {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionUsesLoopVariable { name } = self;
        format!("Function definition does not bind loop variable `{name}`")
    }
}

#[derive(Default)]
struct LoadedNamesVisitor<'a> {
    loaded: Vec<&'a ast::ExprName>,
    stored: Vec<&'a ast::ExprName>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for LoadedNamesVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(name) => match &name.ctx {
                ExprContext::Load => self.loaded.push(name),
                ExprContext::Store => self.stored.push(name),
                _ => {}
            },
            _ => visitor::walk_expr(self, expr),
        }
    }
}

#[derive(Default)]
struct SuspiciousVariablesVisitor<'a> {
    names: Vec<&'a ast::ExprName>,
    safe_functions: Vec<&'a Expr>,
}

/// `Visitor` to collect all suspicious variables (those referenced in
/// functions, but not bound as arguments).
impl<'a> Visitor<'a> for SuspiciousVariablesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef {
                parameters, body, ..
            }) => {
                // Collect all loaded variable names.
                let mut visitor = LoadedNamesVisitor::default();
                visitor.visit_body(body);

                // Treat any non-arguments as "suspicious".
                self.names
                    .extend(visitor.loaded.into_iter().filter(|loaded| {
                        if visitor.stored.iter().any(|stored| stored.id == loaded.id) {
                            return false;
                        }

                        if parameters.includes(&loaded.id) {
                            return false;
                        }

                        true
                    }));

                return;
            }
            Stmt::Return(ast::StmtReturn {
                value: Some(value),
                range: _,
            }) => {
                // Mark `return lambda: x` as safe.
                if value.is_lambda_expr() {
                    self.safe_functions.push(value);
                }
            }
            _ => {}
        }
        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Call(ast::ExprCall {
                func,
                arguments,
                range: _,
            }) => {
                match func.as_ref() {
                    Expr::Name(ast::ExprName { id, .. }) => {
                        if matches!(id.as_str(), "filter" | "reduce" | "map") {
                            for arg in &*arguments.args {
                                if arg.is_lambda_expr() {
                                    self.safe_functions.push(arg);
                                }
                            }
                        }
                    }
                    Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                        if attr == "reduce" {
                            if let Expr::Name(ast::ExprName { id, .. }) = value.as_ref() {
                                if id == "functools" {
                                    for arg in &*arguments.args {
                                        if arg.is_lambda_expr() {
                                            self.safe_functions.push(arg);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }

                for keyword in &*arguments.keywords {
                    if keyword.arg.as_ref().is_some_and(|arg| arg == "key")
                        && keyword.value.is_lambda_expr()
                    {
                        self.safe_functions.push(&keyword.value);
                    }
                }
            }
            Expr::Lambda(ast::ExprLambda {
                parameters,
                body,
                range: _,
            }) => {
                if !self.safe_functions.contains(&expr) {
                    // Collect all loaded variable names.
                    let mut visitor = LoadedNamesVisitor::default();
                    visitor.visit_expr(body);

                    // Treat any non-arguments as "suspicious".
                    self.names
                        .extend(visitor.loaded.into_iter().filter(|loaded| {
                            if visitor.stored.iter().any(|stored| stored.id == loaded.id) {
                                return false;
                            }

                            if parameters
                                .as_ref()
                                .is_some_and(|parameters| parameters.includes(&loaded.id))
                            {
                                return false;
                            }

                            true
                        }));

                    return;
                }
            }
            _ => {}
        }
        visitor::walk_expr(self, expr);
    }
}

#[derive(Default)]
struct NamesFromAssignmentsVisitor<'a> {
    names: Vec<&'a str>,
}

/// `Visitor` to collect all names used in an assignment expression.
impl<'a> Visitor<'a> for NamesFromAssignmentsVisitor<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Name(ast::ExprName { id, .. }) => {
                self.names.push(id.as_str());
            }
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.visit_expr(value);
            }
            Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                for expr in elts {
                    self.visit_expr(expr);
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct AssignedNamesVisitor<'a> {
    names: Vec<&'a str>,
}

/// `Visitor` to collect all used identifiers in a statement.
impl<'a> Visitor<'a> for AssignedNamesVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if stmt.is_function_def_stmt() {
            // Don't recurse.
            return;
        }

        match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                for expr in targets {
                    visitor.visit_expr(expr);
                }
                self.names.extend(visitor.names);
            }
            Stmt::AugAssign(ast::StmtAugAssign { target, .. })
            | Stmt::AnnAssign(ast::StmtAnnAssign { target, .. })
            | Stmt::For(ast::StmtFor { target, .. }) => {
                let mut visitor = NamesFromAssignmentsVisitor::default();
                visitor.visit_expr(target);
                self.names.extend(visitor.names);
            }
            _ => {}
        }

        visitor::walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if expr.is_lambda_expr() {
            // Don't recurse.
            return;
        }

        visitor::walk_expr(self, expr);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        let mut visitor = NamesFromAssignmentsVisitor::default();
        visitor.visit_expr(&comprehension.target);
        self.names.extend(visitor.names);

        visitor::walk_comprehension(self, comprehension);
    }
}

/// B023
pub(crate) fn function_uses_loop_variable(checker: &Checker, node: &Node) {
    // Identify any "suspicious" variables. These are defined as variables that are
    // referenced in a function or lambda body, but aren't bound as arguments.
    let suspicious_variables = {
        let mut visitor = SuspiciousVariablesVisitor::default();
        match node {
            Node::Stmt(stmt) => visitor.visit_stmt(stmt),
            Node::Expr(expr) => visitor.visit_expr(expr),
        }
        visitor.names
    };

    if !suspicious_variables.is_empty() {
        // Identify any variables that are assigned in the loop (ignoring functions).
        let reassigned_in_loop = {
            let mut visitor = AssignedNamesVisitor::default();
            match node {
                Node::Stmt(stmt) => visitor.visit_stmt(stmt),
                Node::Expr(expr) => visitor.visit_expr(expr),
            }
            visitor.names
        };

        // If a variable was used in a function or lambda body, and assigned in the
        // loop, flag it.
        for name in suspicious_variables {
            if reassigned_in_loop.contains(&name.id.as_str()) {
                if checker.insert_flake8_bugbear_range(name.range()) {
                    checker.report_diagnostic(Diagnostic::new(
                        FunctionUsesLoopVariable {
                            name: name.id.to_string(),
                        },
                        name.range(),
                    ));
                }
            }
        }
    }
}
