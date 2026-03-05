use ruff_python_ast::{self as ast, Expr, ExprContext, Identifier, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_codegen::Generator;
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};
use unicode_normalization::UnicodeNormalization;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `delattr` that take a constant attribute value as an
/// argument (e.g., `delattr(obj, "foo")`).
///
/// ## Why is this bad?
/// `delattr` is used to delete attributes dynamically. If the attribute is
/// defined as a constant, it is no safer than a typical property deletion.
/// When possible, prefer `del` statements over `delattr` calls, as the
/// former is more concise and idiomatic.
///
/// ## Example
/// ```python
/// delattr(obj, "foo")
/// ```
///
/// Use instead:
/// ```python
/// del obj.foo
/// ```
///
/// ## Fix safety
/// The fix is marked as unsafe for attribute names that are not in NFKC
/// (Normalization Form KC) normalization. Python normalizes identifiers using
/// NFKC when using attribute access syntax (e.g., `del obj.attr`), but does
/// not normalize string arguments passed to `delattr`. Rewriting
/// `delattr(obj, "ſ")` to `del obj.ſ` would be interpreted as `del obj.s`
/// at runtime, changing behavior.
///
/// Additionally, the fix is marked as unsafe if the expression contains
/// comments, as the replacement may remove comments attached to the original
/// `delattr` call.
///
/// ## References
/// - [Python documentation: `delattr`](https://docs.python.org/3/library/functions.html#delattr)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.4")]
pub(crate) struct DelAttrWithConstant;

impl AlwaysFixableViolation for DelAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Do not call `delattr` with a constant attribute value. It is not any safer than \
             normal property deletion."
            .to_string()
    }

    fn fix_title(&self) -> String {
        "Replace `delattr` with `del` statement".to_string()
    }
}

fn deletion(obj: &Expr, name: &str, generator: Generator) -> String {
    let stmt = Stmt::Delete(ast::StmtDelete {
        targets: vec![Expr::Attribute(ast::ExprAttribute {
            value: Box::new(obj.clone()),
            attr: Identifier::new(name.to_string(), TextRange::default()),
            ctx: ExprContext::Del,
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        })],
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    });
    generator.stmt(&stmt)
}

/// B043
pub(crate) fn delattr_with_constant(checker: &Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let [obj, name] = args else {
        return;
    };
    if obj.is_starred_expr() {
        return;
    }
    let Expr::StringLiteral(ast::ExprStringLiteral { value: name, .. }) = name else {
        return;
    };
    if !is_identifier(name.to_str()) {
        return;
    }
    // Ignore if the attribute name is `__debug__`. Deleting the `__debug__` property is a
    // `SyntaxError`.
    if name.to_str() == "__debug__" {
        return;
    }
    if is_mangled_private(name.to_str()) {
        return;
    }
    if !checker.semantic().match_builtin_expr(func, "delattr") {
        return;
    }

    let attr_name = name.to_str();
    let has_comments = checker.comment_ranges().intersects(expr.range());
    let is_unsafe = attr_name.nfkc().collect::<String>() != attr_name || has_comments;

    // We can only replace a `delattr` call (which is an `Expr`) with a `del`
    // statement (which is a `Stmt`) if the `Expr` is already being used as a
    // `Stmt` (i.e., it's directly within an `Stmt::Expr`).
    if let Stmt::Expr(ast::StmtExpr {
        value: child,
        range: _,
        node_index: _,
    }) = checker.semantic().current_statement()
    {
        if expr == child.as_ref() {
            let mut diagnostic = checker.report_diagnostic(DelAttrWithConstant, expr.range());
            let edit = Edit::range_replacement(
                deletion(obj, name.to_str(), checker.generator()),
                expr.range(),
            );
            let fix = if is_unsafe {
                Fix::unsafe_edit(edit)
            } else {
                Fix::safe_edit(edit)
            };
            diagnostic.set_fix(fix);
        }
    }
}
