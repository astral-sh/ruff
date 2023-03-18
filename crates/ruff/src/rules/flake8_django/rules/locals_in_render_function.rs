use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
/// def index(request):
///     posts = Post.objects.all()
///     return render(request, "app/index.html", locals())
/// ```
///
/// Use instead:
/// ```python
/// from django.shortcuts import render
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
pub fn locals_in_render_function(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["django", "shortcuts", "render"]
        })
    {
        return;
    }

    let locals = if args.len() >= 3 {
        if !is_locals_call(checker, &args[2]) {
            return;
        }
        &args[2]
    } else if let Some(keyword) = keywords.iter().find(|keyword| {
        keyword
            .node
            .arg
            .as_ref()
            .map_or(false, |arg| arg == "context")
    }) {
        if !is_locals_call(checker, &keyword.node.value) {
            return;
        }
        &keyword.node.value
    } else {
        return;
    };

    checker.diagnostics.push(Diagnostic::new(
        DjangoLocalsInRenderFunction,
        Range::from(locals),
    ));
}

fn is_locals_call(checker: &Checker, expr: &Expr) -> bool {
    let ExprKind::Call { func, .. } = &expr.node else {
        return false
    };
    checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["", "locals"])
}
