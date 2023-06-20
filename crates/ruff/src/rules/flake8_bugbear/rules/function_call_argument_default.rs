use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, ArgWithDefault, Arguments, Expr, Ranged};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{compose_call_path, from_qualified_name, CallPath};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::typing::{is_immutable_func, is_mutable_func};
use ruff_python_semantic::SemanticModel;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for function calls in default function arguments.
///
/// ## Why is this bad?
/// Any function call that's used in a default argument will only be performed
/// once, at definition time. The returned value will then be reused by all
/// calls to the function, which can lead to unexpected behaviour.
///
/// ## Example
/// ```python
/// def create_list() -> list[int]:
///     return [1, 2, 3]
///
///
/// def mutable_default(arg: list[int] = create_list()) -> list[int]:
///     arg.append(4)
///     return arg
/// ```
///
/// Use instead:
/// ```python
/// def better(arg: list[int] | None = None) -> list[int]:
///     if arg is None:
///         arg = create_list()
///
///     arg.append(4)
///     return arg
/// ```
///
/// ## Options
/// - `flake8-bugbear.extend-immutable-calls`
#[violation]
pub struct FunctionCallInDefaultArgument {
    pub name: Option<String>,
}

impl Violation for FunctionCallInDefaultArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let FunctionCallInDefaultArgument { name } = self;
        if let Some(name) = name {
            format!("Do not perform function call `{name}` in argument defaults")
        } else {
            format!("Do not perform function call in argument defaults")
        }
    }
}

struct ArgumentDefaultVisitor<'a> {
    semantic: &'a SemanticModel<'a>,
    extend_immutable_calls: Vec<CallPath<'a>>,
    diagnostics: Vec<(DiagnosticKind, TextRange)>,
}

impl<'a> ArgumentDefaultVisitor<'a> {
    fn new(model: &'a SemanticModel<'a>, extend_immutable_calls: Vec<CallPath<'a>>) -> Self {
        Self {
            semantic: model,
            extend_immutable_calls,
            diagnostics: Vec::new(),
        }
    }
}

impl<'a, 'b> Visitor<'b> for ArgumentDefaultVisitor<'b>
where
    'b: 'a,
{
    fn visit_expr(&mut self, expr: &'b Expr) {
        match expr {
            Expr::Call(ast::ExprCall { func, .. }) => {
                if !is_mutable_func(func, self.semantic)
                    && !is_immutable_func(func, self.semantic, &self.extend_immutable_calls)
                {
                    self.diagnostics.push((
                        FunctionCallInDefaultArgument {
                            name: compose_call_path(func),
                        }
                        .into(),
                        expr.range(),
                    ));
                }
                visitor::walk_expr(self, expr);
            }
            Expr::Lambda(_) => {}
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// B008
pub(crate) fn function_call_argument_default(checker: &mut Checker, arguments: &Arguments) {
    // Map immutable calls to (module, member) format.
    let extend_immutable_calls: Vec<CallPath> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| from_qualified_name(target))
        .collect();
    let diagnostics = {
        let mut visitor = ArgumentDefaultVisitor::new(checker.semantic(), extend_immutable_calls);
        for ArgWithDefault {
            default,
            def: _,
            range: _,
        } in arguments
            .posonlyargs
            .iter()
            .chain(&arguments.args)
            .chain(&arguments.kwonlyargs)
        {
            if let Some(expr) = &default {
                visitor.visit_expr(expr);
            }
        }
        visitor.diagnostics
    };
    for (check, range) in diagnostics {
        checker.diagnostics.push(Diagnostic::new(check, range));
    }
}
