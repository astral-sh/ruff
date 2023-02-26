use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use crate::{checkers::ast::Checker, registry::Diagnostic, violation::Violation, Range};

define_violation!(
    /// ## What it does
    /// Checks for use of `locals()` in `render` functions.
    ///
    /// ## Why is this bad?
    /// It could potentially expose variables that you don't want to expose.
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
    pub struct LocalsInRenderFunction;
);
impl Violation for LocalsInRenderFunction {
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
    let diagnostic = {
        let call_path = checker.resolve_call_path(func);
        if call_path.as_ref().map_or(false, |call_path| {
            *call_path.as_slice() == ["django", "shortcuts", "render"]
        }) {
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
            Diagnostic::new(LocalsInRenderFunction, Range::from_located(locals))
        } else {
            return;
        }
    };

    checker.diagnostics.push(diagnostic);
}

fn is_locals_call(checker: &Checker, expr: &Expr) -> bool {
    let ExprKind::Call { func, .. } = &expr.node else {
        return false
    };
    let call_path = checker.resolve_call_path(func);
    call_path
        .as_ref()
        .map_or(false, |call_path| *call_path.as_slice() == ["", "locals"])
}
