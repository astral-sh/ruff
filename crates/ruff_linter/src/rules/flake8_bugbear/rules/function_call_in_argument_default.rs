use ruff_python_ast::{self as ast, Expr, Parameters};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::typing::{
    is_immutable_annotation, is_immutable_func, is_immutable_newtype_call, is_mutable_func,
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
/// Parameters with immutable type annotations will be ignored by this rule.
/// Those whose default arguments are `NewType` calls where the original type
/// is immutable are also ignored.
///
/// Calls and types outside of the standard library can be marked as an exception
/// to this rule with the [`lint.flake8-bugbear.extend-immutable-calls`] configuration option.
///
/// ## Example
///
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
///
/// ```python
/// def better(arg: list[int] | None = None) -> list[int]:
///     if arg is None:
///         arg = create_list()
///
///     arg.append(4)
///     return arg
/// ```
///
/// If the use of a singleton is intentional, assign the result call to a
/// module-level variable, and use that variable in the default argument:
///
/// ```python
/// ERROR = ValueError("Hosts weren't successfully added")
///
///
/// def add_host(error: Exception = ERROR) -> None: ...
/// ```
///
/// ## Options
/// - `lint.flake8-bugbear.extend-immutable-calls`
#[derive(ViolationMetadata)]
pub(crate) struct FunctionCallInDefaultArgument {
    name: Option<String>,
}

impl Violation for FunctionCallInDefaultArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(name) = &self.name {
            format!("Do not perform function call `{name}` in argument defaults; instead, perform the call within the function, or read the default from a module-level singleton variable")
        } else {
            "Do not perform function call in argument defaults; instead, perform the call within the function, or read the default from a module-level singleton variable".to_string()
        }
    }
}

struct ArgumentDefaultVisitor<'a, 'b> {
    semantic: &'a SemanticModel<'b>,
    extend_immutable_calls: &'a [QualifiedName<'b>],
    diagnostics: Vec<(DiagnosticKind, TextRange)>,
}

impl<'a, 'b> ArgumentDefaultVisitor<'a, 'b> {
    fn new(
        semantic: &'a SemanticModel<'b>,
        extend_immutable_calls: &'a [QualifiedName<'b>],
    ) -> Self {
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
                    && !func.as_name_expr().is_some_and(|name| {
                        is_immutable_newtype_call(name, self.semantic, self.extend_immutable_calls)
                    })
                {
                    self.diagnostics.push((
                        FunctionCallInDefaultArgument {
                            name: UnqualifiedName::from_expr(func).map(|name| name.to_string()),
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
pub(crate) fn function_call_in_argument_default(checker: &Checker, parameters: &Parameters) {
    // Map immutable calls to (module, member) format.
    let extend_immutable_calls: Vec<QualifiedName> = checker
        .settings
        .flake8_bugbear
        .extend_immutable_calls
        .iter()
        .map(|target| QualifiedName::from_dotted_name(target))
        .collect();

    let mut visitor = ArgumentDefaultVisitor::new(checker.semantic(), &extend_immutable_calls);
    for parameter in parameters.iter_non_variadic_params() {
        if let Some(default) = parameter.default() {
            if !parameter.annotation().is_some_and(|expr| {
                is_immutable_annotation(expr, checker.semantic(), &extend_immutable_calls)
            }) {
                visitor.visit_expr(default);
            }
        }
    }

    for (check, range) in visitor.diagnostics {
        checker.report_diagnostic(Diagnostic::new(check, range));
    }
}
