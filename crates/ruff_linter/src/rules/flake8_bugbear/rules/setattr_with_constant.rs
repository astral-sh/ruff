use ruff_python_ast::{self as ast, Expr, ExprContext, Identifier, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Generator;
use ruff_python_stdlib::identifiers::{is_identifier, is_mangled_private};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `setattr` that take a constant attribute value as an
/// argument (e.g., `setattr(obj, "foo", 42)`).
///
/// ## Why is this bad?
/// `setattr` is used to set attributes dynamically. If the attribute is
/// defined as a constant, it is no safer than a typical property access. When
/// possible, prefer property access over `setattr` calls, as the former is
/// more concise and idiomatic.
///
/// ## Example
/// ```python
/// setattr(obj, "foo", 42)
/// ```
///
/// Use instead:
/// ```python
/// obj.foo = 42
/// ```
///
/// ## References
/// - [Python documentation: `setattr`](https://docs.python.org/3/library/functions.html#setattr)
#[violation]
pub struct SetAttrWithConstant;

impl AlwaysFixableViolation for SetAttrWithConstant {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Do not call `setattr` with a constant attribute value. It is not any safer than \
             normal property access."
        )
    }

    fn fix_title(&self) -> String {
        "Replace `setattr` with assignment".to_string()
    }
}

fn assignment(obj: &Expr, name: &str, value: &Expr, generator: Generator) -> String {
    let stmt = Stmt::Assign(ast::StmtAssign {
        targets: vec![Expr::Attribute(ast::ExprAttribute {
            value: Box::new(obj.clone()),
            attr: Identifier::new(name.to_string(), TextRange::default()),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        })],
        value: Box::new(value.clone()),
        range: TextRange::default(),
    });
    generator.stmt(&stmt)
}

/// B010
pub(crate) fn setattr_with_constant(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let [obj, name, value] = args else {
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
    if is_mangled_private(name.to_str()) {
        return;
    }
    if !checker.semantic().match_builtin_expr(func, "setattr") {
        return;
    }

    // We can only replace a `setattr` call (which is an `Expr`) with an assignment
    // (which is a `Stmt`) if the `Expr` is already being used as a `Stmt`
    // (i.e., it's directly within an `Stmt::Expr`).
    if let Stmt::Expr(ast::StmtExpr {
        value: child,
        range: _,
    }) = checker.semantic().current_statement()
    {
        if expr == child.as_ref() {
            let mut diagnostic = Diagnostic::new(SetAttrWithConstant, expr.range());
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                assignment(obj, name.to_str(), value, checker.generator()),
                expr.range(),
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}
