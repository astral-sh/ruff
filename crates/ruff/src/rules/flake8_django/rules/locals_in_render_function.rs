use rustpython_parser::ast::{self, Expr, Keyword, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::SemanticModel;

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
#[violation]
pub struct DjangoLocalsInRenderFunction;

impl Violation for DjangoLocalsInRenderFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid passing `locals()` as context to a `render` function")
    }
}

/// DJ003
pub(crate) fn locals_in_render_function(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !checker
        .semantic()
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            matches!(call_path.as_slice(), ["django", "shortcuts", "render"])
        })
    {
        return;
    }

    let locals = if args.len() >= 3 {
        if !is_locals_call(&args[2], checker.semantic()) {
            return;
        }
        &args[2]
    } else if let Some(keyword) = keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().map_or(false, |arg| arg == "context"))
    {
        if !is_locals_call(&keyword.value, checker.semantic()) {
            return;
        }
        &keyword.value
    } else {
        return;
    };

    checker.diagnostics.push(Diagnostic::new(
        DjangoLocalsInRenderFunction,
        locals.range(),
    ));
}

fn is_locals_call(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    semantic.resolve_call_path(func).map_or(false, |call_path| {
        matches!(call_path.as_slice(), ["", "locals"])
    })
}
