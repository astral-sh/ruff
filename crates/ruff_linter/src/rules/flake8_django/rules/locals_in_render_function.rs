use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of `locals()` in `render` functions.
///
/// ## Why is this bad?
/// Using `locals()` can expose internal variables or other unintentional
/// data to the rendered template.
///
/// ## Example
/// ```python
/// from django.shortcuts import render
///
///
/// def index(request):
///     posts = Post.objects.all()
///     return render(request, "app/index.html", locals())
/// ```
///
/// Use instead:
/// ```python
/// from django.shortcuts import render
///
///
/// def index(request):
///     posts = Post.objects.all()
///     context = {"posts": posts}
///     return render(request, "app/index.html", context)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.253")]
pub(crate) struct DjangoLocalsInRenderFunction;

impl Violation for DjangoLocalsInRenderFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Avoid passing `locals()` as context to a `render` function".to_string()
    }
}

/// DJ003
pub(crate) fn locals_in_render_function(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::DJANGO) {
        return;
    }

    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["django", "shortcuts", "render"])
        })
    {
        return;
    }

    if let Some(argument) = call.arguments.find_argument_value("context", 2) {
        if is_locals_call(argument, checker.semantic()) {
            checker.report_diagnostic(DjangoLocalsInRenderFunction, argument.range());
        }
    }
}

fn is_locals_call(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    semantic.match_builtin_expr(func, "locals")
}
