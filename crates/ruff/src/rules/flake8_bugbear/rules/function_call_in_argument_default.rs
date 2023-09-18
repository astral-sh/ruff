use ruff_python_ast::{self as ast, Expr, ParameterWithDefault, Parameters};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{compose_call_path, from_qualified_name, CallPath};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::typing::{
    is_immutable_annotation, is_immutable_func, is_mutable_func,
};
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
/// Calls can be marked as an exception to this rule with the
/// [`flake8-bugbear.extend-immutable-calls`] configuration option.
///
/// Arguments with immutable type annotations will be ignored by this rule.
/// Types outside of the standard library can be marked as immutable with the
/// [`flake8-bugbear.extend-immutable-calls`] configuration option as well.
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
    name: Option<String>,
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

struct ArgumentDefaultVisitor<'a, 'b> {
    semantic: &'a SemanticModel<'b>,
    extend_immutable_calls: &'a [CallPath<'b>],
    diagnostics: Vec<(DiagnosticKind, TextRange)>,
}

impl<'a, 'b> ArgumentDefaultVisitor<'a, 'b> {
    fn new(semantic: &'a SemanticModel<'b>, extend_immutable_calls: &'a [CallPath<'b>]) -> Self {
        Self {
            semantic,
            extend_immutable_calls,
            diagnostics: Vec::new(),
        }
    }
}

impl Visitor<'_> for ArgumentDefaultVisitor<'_, '_> {
    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call(ast::ExprCall { func, .. }) => {
                if !is_mutable_func(func, self.semantic)
                    && !is_immutable_func(func, self.semantic, self.extend_immutable_calls)
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
            Expr::Lambda(_) => {
                // Don't recurse.
            }
            _ => visitor::walk_expr(self, expr),
        }
    }
}

/// B008
pub(crate) fn function_call_in_argument_default(checker: &mut Checker, parameters: &Parameters) {
    // Map immutable calls to (module, member) format.
    let extend_immutable_calls: Vec<CallPath> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| from_qualified_name(target))
        .collect();

    let mut visitor = ArgumentDefaultVisitor::new(checker.semantic(), &extend_immutable_calls);
    for ParameterWithDefault {
        default,
        parameter,
        range: _,
    } in parameters
        .posonlyargs
        .iter()
        .chain(&parameters.args)
        .chain(&parameters.kwonlyargs)
    {
        if let Some(expr) = &default {
            if !parameter.annotation.as_ref().is_some_and(|expr| {
                is_immutable_annotation(expr, checker.semantic(), &extend_immutable_calls)
            }) {
                visitor.visit_expr(expr);
            }
        }
    }

    for (check, range) in visitor.diagnostics {
        checker.diagnostics.push(Diagnostic::new(check, range));
    }
}
