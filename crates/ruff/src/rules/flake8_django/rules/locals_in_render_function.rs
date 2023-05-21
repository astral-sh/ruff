use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::traits::AstAnalyzer;
use crate::checkers::ast::RuleContext;
use crate::registry::Rule;

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
impl AstAnalyzer<ast::ExprCall> for DjangoLocalsInRenderFunction {
    fn rule() -> Rule {
        Rule::DjangoLocalsInRenderFunction
    }

    fn run(diagnostics: &mut Vec<Diagnostic>, context: &RuleContext, node: &ast::ExprCall) {
        locals_in_render_function(diagnostics, context, node);
    }
}

pub(crate) fn locals_in_render_function(
    diagnostics: &mut Vec<Diagnostic>,
    checker: &RuleContext,
    ast::ExprCall {
        func,
        args,
        keywords,
        ..
    }: &ast::ExprCall,
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
    } else if let Some(keyword) = keywords
        .iter()
        .find(|keyword| keyword.arg.as_ref().map_or(false, |arg| arg == "context"))
    {
        if !is_locals_call(checker, &keyword.value) {
            return;
        }
        &keyword.value
    } else {
        return;
    };

    diagnostics.push(Diagnostic::new(
        DjangoLocalsInRenderFunction,
        locals.range(),
    ));
}

fn is_locals_call(checker: &RuleContext, expr: &Expr) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false
    };
    checker
        .ctx
        .resolve_call_path(func)
        .map_or(false, |call_path| call_path.as_slice() == ["", "locals"])
}
