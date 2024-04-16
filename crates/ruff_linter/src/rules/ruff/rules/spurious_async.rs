use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::identifier::Identifier;
use ruff_python_ast::visitor::preorder;
use ruff_python_ast::{self as ast, AnyNodeRef, Expr, Stmt};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for functions declared `async` that do not await or otherwise use features requiring the
/// function to be declared `async`.
///
/// ## Why is this bad?
/// Declaring a function `async` when it's not is usually a mistake, and will artificially limit the
/// contexts where that function may be called. In some cases, labeling a function `async` is
/// semantically meaningful (e.g. with the trio library).
///
/// ## Examples
/// ```python
/// async def foo():
///     bar()
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     bar()
/// ```
#[violation]
pub struct SpuriousAsync {
    name: String,
}

impl Violation for SpuriousAsync {
    #[derive_message_formats]
    fn message(&self) -> String {
        let SpuriousAsync { name } = self;
        format!("Function `{name}` is declared `async`, but doesn't await or use async features.")
    }
}

#[derive(Default)]
struct AsyncExprVisitor {
    found_await_or_async: bool,
}

/// Traverse a function's body to find whether it contains an await-expr, an async-with, or an
/// async-for. Stop traversing after one is found. The bodies of inner-functions and inner-classes
/// aren't traversed.
impl<'a> preorder::PreorderVisitor<'a> for AsyncExprVisitor {
    fn enter_node(&mut self, _node: AnyNodeRef<'a>) -> preorder::TraversalSignal {
        if self.found_await_or_async {
            preorder::TraversalSignal::Skip
        } else {
            preorder::TraversalSignal::Traverse
        }
    }
    fn visit_expr(&mut self, expr: &'a Expr) {
        match expr {
            Expr::Await(_) => {
                self.found_await_or_async = true;
            }
            _ => preorder::walk_expr(self, expr),
        }
    }
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                self.found_await_or_async = true;
            }
            Stmt::For(ast::StmtFor { is_async: true, .. }) => {
                self.found_await_or_async = true;
            }
            // avoid counting inner classes' or functions' bodies toward the search
            Stmt::FunctionDef(function_def) => {
                function_def_visit_preorder_except_body(function_def, self);
            }
            Stmt::ClassDef(class_def) => class_def_visit_preorder_except_body(class_def, self),
            _ => preorder::walk_stmt(self, stmt),
        }
    }
}

/// Very nearly `crate::node::StmtFunctionDef.visit_preorder`, except it is specialized and,
/// crucially, doesn't traverse the body.
fn function_def_visit_preorder_except_body<'a, V>(
    function_def: &'a ast::StmtFunctionDef,
    visitor: &mut V,
) where
    V: preorder::PreorderVisitor<'a>,
{
    let ast::StmtFunctionDef {
        parameters,
        decorator_list,
        returns,
        type_params,
        ..
    } = function_def;

    for decorator in decorator_list {
        visitor.visit_decorator(decorator);
    }

    if let Some(type_params) = type_params {
        visitor.visit_type_params(type_params);
    }

    visitor.visit_parameters(parameters);

    for expr in returns {
        visitor.visit_annotation(expr);
    }
}

/// Very nearly `crate::node::StmtClassDef.visit_preorder`, except it is specialized and,
/// crucially, doesn't traverse the body.
fn class_def_visit_preorder_except_body<'a, V>(class_def: &'a ast::StmtClassDef, visitor: &mut V)
where
    V: preorder::PreorderVisitor<'a>,
{
    let ast::StmtClassDef {
        arguments,
        decorator_list,
        type_params,
        ..
    } = class_def;

    for decorator in decorator_list {
        visitor.visit_decorator(decorator);
    }

    if let Some(type_params) = type_params {
        visitor.visit_type_params(type_params);
    }

    if let Some(arguments) = arguments {
        visitor.visit_arguments(arguments);
    }
}

/// RUF029
pub(crate) fn spurious_async(
    checker: &mut Checker,
    function_def @ ast::StmtFunctionDef {
        is_async,
        name,
        body,
        ..
    }: &ast::StmtFunctionDef,
) {
    if !is_async {
        return;
    }

    let found_await_or_async = {
        let mut visitor = AsyncExprVisitor::default();
        preorder::walk_body(&mut visitor, body);
        visitor.found_await_or_async
    };

    if !found_await_or_async {
        checker.diagnostics.push(Diagnostic::new(
            SpuriousAsync {
                name: name.to_string(),
            },
            function_def.identifier(),
        ));
    }
}
