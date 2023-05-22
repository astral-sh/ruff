use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Arguments, Constant, Expr, Ranged};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::call_path::{compose_call_path, from_qualified_name, CallPath};
use ruff_python_ast::visitor;
use ruff_python_ast::visitor::Visitor;
use ruff_python_semantic::analyze::typing::is_immutable_func;
use ruff_python_semantic::model::SemanticModel;

use crate::checkers::ast::Checker;
use crate::rules::flake8_bugbear::rules::mutable_argument_default::is_mutable_func;

/// ## What it does
/// Checks for function calls in default function arguments.
///
/// ## Why is this bad?
/// Any function call that's used in a default argument will only be performed
/// once, at definition time. The returned value will then be reused by all
/// calls to the function, which can lead to unexpected behaviour.
///
/// ## Options
/// - `flake8-bugbear.extend-immutable-calls`
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
/// Alternatively, if shared behavior is desirable, clarify the intent by
/// assigning to a module-level variable:
/// ```python
/// I_KNOW_THIS_IS_SHARED_STATE = create_list()
///
///
/// def mutable_default(arg: list[int] = I_KNOW_THIS_IS_SHARED_STATE) -> list[int]:
///     arg.append(4)
///     return arg
/// ```
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
    model: &'a SemanticModel<'a>,
    extend_immutable_calls: Vec<CallPath<'a>>,
    diagnostics: Vec<(DiagnosticKind, TextRange)>,
}

impl<'a> ArgumentDefaultVisitor<'a> {
    fn new(model: &'a SemanticModel<'a>, extend_immutable_calls: Vec<CallPath<'a>>) -> Self {
        Self {
            model,
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
            Expr::Call(ast::ExprCall { func, args, .. }) => {
                if !is_mutable_func(self.model, func)
                    && !is_immutable_func(self.model, func, &self.extend_immutable_calls)
                    && !is_nan_or_infinity(func, args)
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

fn is_nan_or_infinity(expr: &Expr, args: &[Expr]) -> bool {
    let Expr::Name(ast::ExprName { id, .. }) = expr else {
        return false;
    };
    if id != "float" {
        return false;
    }
    let Some(arg) = args.first() else {
        return false;
    };
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    } )= arg else {
        return false;
    };
    let lowercased = value.to_lowercase();
    matches!(
        lowercased.as_str(),
        "nan" | "+nan" | "-nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
    )
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
        let mut visitor =
            ArgumentDefaultVisitor::new(checker.semantic_model(), extend_immutable_calls);
        for expr in arguments
            .defaults
            .iter()
            .chain(arguments.kw_defaults.iter())
        {
            visitor.visit_expr(expr);
        }
        visitor.diagnostics
    };
    for (check, range) in diagnostics {
        checker.diagnostics.push(Diagnostic::new(check, range));
    }
}
