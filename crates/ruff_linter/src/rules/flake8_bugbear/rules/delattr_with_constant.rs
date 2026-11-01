use ruff_python_ast::{
    Builder, Expr, ExprContext, ExprFields, ExprStringLiteralFields, NodeList, StmtKind, build,
};
use ruff_text_size::{Ranged, TextRange};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_codegen::Generator;
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};
use unicode_normalization::UnicodeNormalization;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_bugbear::helpers::attribute_target;
use crate::{AlwaysFixableViolation, Applicability, Edit, Fix};

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
#[violation_metadata(preview_since = "0.15.6", category = Category::Complexity)]
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

/// B043
pub(crate) fn delattr_with_constant(
    checker: &Checker,
    &expr: &Expr<'_>,
    func: Expr<'_>,
    args: NodeList<'_, ruff_python_ast::kind::Expr>,
) {
    if !checker.semantic().match_builtin_expr(func, "delattr") {
        return;
    }

    let Some(
        [
            obj,
            ExprFields::StringLiteral(ExprStringLiteralFields { value: name, .. }),
        ],
    ) = args.as_array().map(|items| items.map(Expr::fields))
    else {
        return;
    };
    if obj.view().is_starred_expr() {
        return;
    }

    let attr_name = name.to_str();

    // Ignore if the attribute name is `__debug__` (syntax error), not a valid
    // identifier (eg, keywords), or otherwise a name that would be mangled.
    if !is_identifier(attr_name) || attr_name == "__debug__" || is_mangled_private(attr_name) {
        return;
    }

    let has_comments = checker.comment_ranges().intersects(expr.range());
    let applicability = if attr_name.nfkc().collect::<String>() != attr_name || has_comments {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    // We can only replace a `delattr` call (which is an `Expr`) with a `del`
    // statement (which is a `Stmt`) if the `Expr` is already being used as a
    // `Stmt` (i.e., it's directly within an `Stmt::Expr`).
    if let StmtKind::Expr(expr_node) = checker.semantic().current_statement().classify() {
        let child = expr_node.value();
        if expr == child {
            let mut diagnostic = checker.report_diagnostic(DelAttrWithConstant, expr.range());
            let edit = Edit::range_replacement(
                generate_del_statement(obj.view(), attr_name, checker.generator()),
                expr.range(),
            );
            diagnostic.set_fix(Fix::applicable_edit(edit, applicability));
        }
    }
}

fn generate_del_statement(obj: Expr<'_>, attr_name: &str, generator: Generator) -> String {
    let mut builder = Builder::default();
    let targets = attribute_target(&mut builder, obj, attr_name, ExprContext::Del);
    let stmt = builder.add(build::StmtDelete {
        targets,
        range: TextRange::default(),
    });
    generator.stmt(builder.get(stmt).into())
}
