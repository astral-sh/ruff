use rustpython_parser::ast::{self, Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{create_expr, create_stmt, unparse_stmt};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::whitespace;

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

/// ## What it does
/// Checks for the use of string literals in exception constructors.
///
/// ## Why is this bad?
/// Python includes the `raise` in the default traceback (and formatters
/// like Rich and IPython do too).
///
/// By using a string literal, the error message will be duplicated in the
/// traceback, which can make the traceback less readable.
///
/// ## Example
/// Given:
/// ```python
/// raise RuntimeError("'Some value' is incorrect")
/// ```
///
/// Python will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 2, in <module>
///     raise RuntimeError("Some value is incorrect")
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// Instead, assign the string to a variable:
/// ```python
/// msg = "'Some value' is incorrect"
/// raise RuntimeError(msg)
/// ```
///
/// Which will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
#[violation]
pub struct RawStringInException;

impl Violation for RawStringInException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a string literal, assign to variable first")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Assign to variable; remove string literal".to_string())
    }
}

/// ## What it does
/// Checks for the use of f-strings in exception constructors.
///
/// ## Why is this bad?
/// Python includes the `raise` in the default traceback (and formatters
/// like Rich and IPython do too).
///
/// By using an f-string, the error message will be duplicated in the
/// traceback, which can make the traceback less readable.
///
/// ## Example
/// Given:
/// ```python
/// sub = "Some value"
/// raise RuntimeError(f"{sub!r} is incorrect")
/// ```
///
/// Python will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 2, in <module>
///     raise RuntimeError(f"{sub!r} is incorrect")
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// Instead, assign the string to a variable:
/// ```python
/// sub = "Some value"
/// msg = f"{sub!r} is incorrect"
/// raise RuntimeError(msg)
/// ```
///
/// Which will produce a traceback like:
/// ```console
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
#[violation]
pub struct FStringInException;

impl Violation for FStringInException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use an f-string literal, assign to variable first")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Assign to variable; remove f-string literal".to_string())
    }
}

/// ## What it does
/// Checks for the use of `.format` calls on string literals in exception
/// constructors.
///
/// ## Why is this bad?
/// Python includes the `raise` in the default traceback (and formatters
/// like Rich and IPython do too).
///
/// By using a `.format` call, the error message will be duplicated in the
/// traceback, which can make the traceback less readable.
///
/// ## Example
/// Given:
/// ```python
/// sub = "Some value"
/// raise RuntimeError("'{}' is incorrect".format(sub))
/// ```
///
/// Python will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 2, in <module>
///     raise RuntimeError("'{}' is incorrect".format(sub))
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// Instead, assign the string to a variable:
/// ```python
/// sub = "Some value"
/// msg = "'{}' is incorrect".format(sub)
/// raise RuntimeError(msg)
/// ```
///
/// Which will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
#[violation]
pub struct DotFormatInException;

impl Violation for DotFormatInException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a `.format()` string directly, assign to variable first")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Assign to variable; remove `.format()` string".to_string())
    }
}

/// Generate the [`Fix`] for EM001, EM002, and EM003 violations.
///
/// This assumes that the violation is fixable and that the patch should
/// be generated. The exception argument should be either a string literal,
/// an f-string, or a `.format` string.
///
/// The fix includes two edits:
/// 1. Insert the exception argument into a variable assignment before the
///   `raise` statement. The variable name is `msg`.
/// 2. Replace the exception argument with the variable name.
fn generate_fix(stylist: &Stylist, stmt: &Stmt, exc_arg: &Expr, indentation: &str) -> Fix {
    let assignment = unparse_stmt(
        &create_stmt(StmtKind::Assign(ast::StmtAssign {
            targets: vec![create_expr(ExprKind::Name(ast::ExprName {
                id: "msg".into(),
                ctx: ExprContext::Store,
            }))],
            value: Box::new(exc_arg.clone()),
            type_comment: None,
        })),
        stylist,
    );
    #[allow(deprecated)]
    Fix::unspecified_edits(
        Edit::insertion(
            format!(
                "{}{}{}",
                assignment,
                stylist.line_ending().as_str(),
                indentation,
            ),
            stmt.start(),
        ),
        [Edit::range_replacement(
            String::from("msg"),
            exc_arg.range(),
        )],
    )
}

/// EM101, EM102, EM103
pub(crate) fn string_in_exception(checker: &mut Checker, stmt: &Stmt, exc: &Expr) {
    if let ExprKind::Call(ast::ExprCall { args, .. }) = &exc.node {
        if let Some(first) = args.first() {
            match &first.node {
                // Check for string literals
                ExprKind::Constant(ast::ExprConstant {
                    value: Constant::Str(string),
                    ..
                }) => {
                    if checker.settings.rules.enabled(Rule::RawStringInException) {
                        if string.len() > checker.settings.flake8_errmsg.max_string_length {
                            let indentation = whitespace::indentation(checker.locator, stmt)
                                .and_then(|indentation| {
                                    if checker.ctx.find_binding("msg").is_none() {
                                        Some(indentation)
                                    } else {
                                        None
                                    }
                                });
                            let mut diagnostic =
                                Diagnostic::new(RawStringInException, first.range());
                            if let Some(indentation) = indentation {
                                if checker.patch(diagnostic.kind.rule()) {
                                    diagnostic.set_fix(generate_fix(
                                        checker.stylist,
                                        stmt,
                                        first,
                                        indentation,
                                    ));
                                }
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
                // Check for f-strings
                ExprKind::JoinedStr(_) => {
                    if checker.settings.rules.enabled(Rule::FStringInException) {
                        let indentation = whitespace::indentation(checker.locator, stmt).and_then(
                            |indentation| {
                                if checker.ctx.find_binding("msg").is_none() {
                                    Some(indentation)
                                } else {
                                    None
                                }
                            },
                        );
                        let mut diagnostic = Diagnostic::new(FStringInException, first.range());
                        if let Some(indentation) = indentation {
                            if checker.patch(diagnostic.kind.rule()) {
                                diagnostic.set_fix(generate_fix(
                                    checker.stylist,
                                    stmt,
                                    first,
                                    indentation,
                                ));
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
                // Check for .format() calls
                ExprKind::Call(ast::ExprCall { func, .. }) => {
                    if checker.settings.rules.enabled(Rule::DotFormatInException) {
                        if let ExprKind::Attribute(ast::ExprAttribute { value, attr, .. }) =
                            &func.node
                        {
                            if attr == "format" && matches!(value.node, ExprKind::Constant(_)) {
                                let indentation = whitespace::indentation(checker.locator, stmt)
                                    .and_then(|indentation| {
                                        if checker.ctx.find_binding("msg").is_none() {
                                            Some(indentation)
                                        } else {
                                            None
                                        }
                                    });
                                let mut diagnostic =
                                    Diagnostic::new(DotFormatInException, first.range());
                                if let Some(indentation) = indentation {
                                    if checker.patch(diagnostic.kind.rule()) {
                                        diagnostic.set_fix(generate_fix(
                                            checker.stylist,
                                            stmt,
                                            first,
                                            indentation,
                                        ));
                                    }
                                }
                                checker.diagnostics.push(diagnostic);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
