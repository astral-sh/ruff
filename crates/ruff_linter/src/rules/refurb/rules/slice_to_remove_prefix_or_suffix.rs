use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, PythonVersion};
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::Locator;

/// ## What it does
/// Checks for code that could be written more idiomatically using
/// [`str.removeprefix()`](https://docs.python.org/3/library/stdtypes.html#str.removeprefix)
/// or [`str.removesuffix()`](https://docs.python.org/3/library/stdtypes.html#str.removesuffix).
///
/// Specifically, the rule flags code that conditionally removes a prefix or suffix
/// using a slice operation following an `if` test that uses `str.startswith()` or `str.endswith()`.
///
/// The rule is only applied if your project targets Python 3.9 or later.
///
/// ## Why is this bad?
/// The methods [`str.removeprefix()`](https://docs.python.org/3/library/stdtypes.html#str.removeprefix)
/// and [`str.removesuffix()`](https://docs.python.org/3/library/stdtypes.html#str.removesuffix),
/// introduced in Python 3.9, have the same behavior while being more readable and efficient.
///
/// ## Example
/// ```python
/// def example(filename: str, text: str):
///     filename = filename[:-4] if filename.endswith(".txt") else filename
///
///     if text.startswith("pre"):
///         text = text[3:]
/// ```
///
/// Use instead:
/// ```python
/// def example(filename: str, text: str):
///     filename = filename.removesuffix(".txt")
///     text = text.removeprefix("pre")
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct SliceToRemovePrefixOrSuffix {
    affix_kind: AffixKind,
    stmt_or_expression: StmtOrExpr,
}

impl AlwaysFixableViolation for SliceToRemovePrefixOrSuffix {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.affix_kind {
            AffixKind::StartsWith => {
                "Prefer `str.removeprefix()` over conditionally replacing with slice.".to_string()
            }
            AffixKind::EndsWith => {
                "Prefer `str.removesuffix()` over conditionally replacing with slice.".to_string()
            }
        }
    }

    fn fix_title(&self) -> String {
        let method_name = self.affix_kind.as_str();
        let replacement = self.affix_kind.replacement();
        let context = match self.stmt_or_expression {
            StmtOrExpr::Statement => "assignment",
            StmtOrExpr::Expression => "ternary expression",
        };
        format!("Use {replacement} instead of {context} conditional upon {method_name}.")
    }
}

/// FURB188
pub(crate) fn slice_to_remove_affix_expr(checker: &Checker, if_expr: &ast::ExprIf) {
    if checker.target_version() < PythonVersion::PY39 {
        return;
    }

    if let Some(removal_data) = affix_removal_data_expr(if_expr) {
        if affix_matches_slice_bound(&removal_data, checker.semantic()) {
            let kind = removal_data.affix_query.kind;
            let text = removal_data.text;

            let mut diagnostic = Diagnostic::new(
                SliceToRemovePrefixOrSuffix {
                    affix_kind: kind,
                    stmt_or_expression: StmtOrExpr::Expression,
                },
                if_expr.range,
            );
            let replacement =
                generate_removeaffix_expr(text, &removal_data.affix_query, checker.locator());

            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                replacement,
                if_expr.start(),
                if_expr.end(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// FURB188
pub(crate) fn slice_to_remove_affix_stmt(checker: &Checker, if_stmt: &ast::StmtIf) {
    if checker.target_version() < PythonVersion::PY39 {
        return;
    }
    if let Some(removal_data) = affix_removal_data_stmt(if_stmt) {
        if affix_matches_slice_bound(&removal_data, checker.semantic()) {
            let kind = removal_data.affix_query.kind;
            let text = removal_data.text;

            let mut diagnostic = Diagnostic::new(
                SliceToRemovePrefixOrSuffix {
                    affix_kind: kind,
                    stmt_or_expression: StmtOrExpr::Statement,
                },
                if_stmt.range,
            );

            let replacement = generate_assignment_with_removeaffix(
                text,
                &removal_data.affix_query,
                checker.locator(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                replacement,
                if_stmt.start(),
                if_stmt.end(),
            )));
            checker.report_diagnostic(diagnostic);
        }
    }
}

/// Given an expression of the form:
///
/// ```python
/// text[slice] if text.func(affix) else text
/// ```
///
/// where `func` is either `startswith` or `endswith`,
/// this function collects `text`,`func`, `affix`, and the non-null
/// bound of the slice. Otherwise, returns `None`.
fn affix_removal_data_expr(if_expr: &ast::ExprIf) -> Option<RemoveAffixData> {
    let ast::ExprIf {
        test,
        body,
        orelse,
        range: _,
    } = if_expr;

    let ast::ExprSubscript { value, slice, .. } = body.as_subscript_expr()?;
    // Variable names correspond to:
    // ```python
    // value[slice] if test else orelse
    // ```
    affix_removal_data(value, test, orelse, slice)
}

/// Given a statement of the form:
///
/// ```python
///  if text.func(affix):
///     text = text[slice]
/// ```
///
/// where `func` is either `startswith` or `endswith`,
/// this function collects `text`,`func`, `affix`, and the non-null
/// bound of the slice. Otherwise, returns `None`.
fn affix_removal_data_stmt(if_stmt: &ast::StmtIf) -> Option<RemoveAffixData> {
    let ast::StmtIf {
        test,
        body,
        elif_else_clauses,
        range: _,
    } = if_stmt;

    // Cannot safely transform, e.g.,
    // ```python
    // if text.startswith(prefix):
    //     text = text[len(prefix):]
    // else:
    //     text = "something completely different"
    // ```
    if !elif_else_clauses.is_empty() {
        return None;
    }

    // Cannot safely transform, e.g.,
    // ```python
    // if text.startswith(prefix):
    //     text = f"{prefix} something completely different"
    //     text = text[len(prefix):]
    // ```
    let [statement] = body.as_slice() else {
        return None;
    };

    // Variable names correspond to:
    // ```python
    // if test:
    //     else_or_target_name = value[slice]
    // ```
    let ast::StmtAssign {
        value,
        targets,
        range: _,
    } = statement.as_assign_stmt()?;
    let [target] = targets.as_slice() else {
        return None;
    };
    let ast::ExprSubscript { value, slice, .. } = value.as_subscript_expr()?;

    affix_removal_data(value, test, target, slice)
}

/// Suppose given a statement of the form:
/// ```python
/// if test:
///     else_or_target_name = value[slice]
/// ```
/// or an expression of the form:
/// ```python
/// value[slice] if test else else_or_target_name
/// ```
/// This function verifies that
///   - `value` and `else_or_target_name`
///     are equal to a common name `text`
///   - `test` is of the form `text.startswith(prefix)`
///     or `text.endswith(suffix)`
///   - `slice` has no upper bound in the case of a prefix,
///     and no lower bound in the case of a suffix
///
/// If these conditions are satisfied, the function
/// returns the corresponding `RemoveAffixData` object;
/// otherwise it returns `None`.
fn affix_removal_data<'a>(
    value: &'a ast::Expr,
    test: &'a ast::Expr,
    else_or_target: &'a ast::Expr,
    slice: &'a ast::Expr,
) -> Option<RemoveAffixData<'a>> {
    let compr_value = ast::comparable::ComparableExpr::from(value);
    let compr_else_or_target = ast::comparable::ComparableExpr::from(else_or_target);
    if compr_value != compr_else_or_target {
        return None;
    }
    let slice = slice.as_slice_expr()?;

    // Exit early if slice step is...
    if slice
        .step
        .as_deref()
        // present and
        .is_some_and(|step| match step {
            // not equal to 1
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(x),
                ..
            }) => x.as_u8() != Some(1),
            // and not equal to `None` or `True`
            ast::Expr::NoneLiteral(_)
            | ast::Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, .. }) => false,
            _ => true,
        })
    {
        return None;
    }

    let compr_test_expr = ast::comparable::ComparableExpr::from(
        &test.as_call_expr()?.func.as_attribute_expr()?.value,
    );
    let func_name = test
        .as_call_expr()?
        .func
        .as_attribute_expr()?
        .attr
        .id
        .as_str();

    let func_args = &test.as_call_expr()?.arguments.args;

    let [affix] = func_args.as_ref() else {
        return None;
    };
    if compr_value != compr_test_expr || compr_test_expr != compr_else_or_target {
        return None;
    }
    let (affix_kind, bound) = match func_name {
        "startswith" if slice.upper.is_none() => (AffixKind::StartsWith, slice.lower.as_ref()?),
        "endswith" if slice.lower.is_none() => (AffixKind::EndsWith, slice.upper.as_ref()?),
        _ => return None,
    };
    Some(RemoveAffixData {
        text: value,
        bound,
        affix_query: AffixQuery {
            kind: affix_kind,
            affix,
        },
    })
}

/// Tests whether the slice of the given string actually removes the
/// detected affix.
///
/// For example, in the situation
///
/// ```python
///  text[:bound] if text.endswith(suffix) else text
/// ```
///
/// This function verifies that `bound == -len(suffix)` in two cases:
///   - `suffix` is a string literal and `bound` is a number literal
///   - `suffix` is an expression and `bound` is
///     exactly `-len(suffix)` (as AST nodes, prior to evaluation.)
fn affix_matches_slice_bound(data: &RemoveAffixData, semantic: &SemanticModel) -> bool {
    let RemoveAffixData {
        text: _,
        bound,
        affix_query: AffixQuery { kind, affix },
    } = *data;

    match (kind, bound, affix) {
        (
            AffixKind::StartsWith,
            ast::Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: num,
                range: _,
            }),
            ast::Expr::StringLiteral(ast::ExprStringLiteral {
                range: _,
                value: string_val,
            }),
        ) => num
            .as_int()
            // Only support prefix removal for size at most `usize::MAX`
            .and_then(ast::Int::as_usize)
            .is_some_and(|x| x == string_val.chars().count()),
        (
            AffixKind::StartsWith,
            ast::Expr::Call(ast::ExprCall {
                range: _,
                func,
                arguments,
            }),
            _,
        ) => {
            arguments.len() == 1
                && arguments.find_positional(0).is_some_and(|arg| {
                    let compr_affix = ast::comparable::ComparableExpr::from(affix);
                    let compr_arg = ast::comparable::ComparableExpr::from(arg);
                    compr_affix == compr_arg
                })
                && semantic.match_builtin_expr(func, "len")
        }
        (
            AffixKind::EndsWith,
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::USub,
                operand,
                range: _,
            }),
            ast::Expr::StringLiteral(ast::ExprStringLiteral {
                range: _,
                value: string_val,
            }),
        ) if operand.is_number_literal_expr() => operand.as_number_literal_expr().is_some_and(
            |ast::ExprNumberLiteral { value, .. }| {
                // Only support prefix removal for size at most `u32::MAX`
                value
                    .as_int()
                    .and_then(ast::Int::as_usize)
                    .is_some_and(|x| x == string_val.chars().count())
            },
        ),
        (
            AffixKind::EndsWith,
            ast::Expr::UnaryOp(ast::ExprUnaryOp {
                op: ast::UnaryOp::USub,
                operand,
                range: _,
            }),
            _,
        ) => operand.as_call_expr().is_some_and(
            |ast::ExprCall {
                 range: _,
                 func,
                 arguments,
             }| {
                arguments.len() == 1
                    && arguments.find_positional(0).is_some_and(|arg| {
                        let compr_affix = ast::comparable::ComparableExpr::from(affix);
                        let compr_arg = ast::comparable::ComparableExpr::from(arg);
                        compr_affix == compr_arg
                    })
                    && semantic.match_builtin_expr(func, "len")
            },
        ),
        _ => false,
    }
}

/// Generates the source code string
/// ```python
/// text = text.removeprefix(prefix)
/// ```
/// or
/// ```python
/// text = text.removesuffix(prefix)
/// ```
/// as appropriate.
fn generate_assignment_with_removeaffix(
    text: &ast::Expr,
    affix_query: &AffixQuery,
    locator: &Locator,
) -> String {
    let text_str = locator.slice(text);
    let affix_str = locator.slice(affix_query.affix);
    let replacement = affix_query.kind.replacement();
    format!("{text_str} = {text_str}.{replacement}({affix_str})")
}

/// Generates the source code string
/// ```python
/// text.removeprefix(prefix)
/// ```
/// or
///
/// ```python
/// text.removesuffix(suffix)
/// ```
/// as appropriate.
fn generate_removeaffix_expr(
    text: &ast::Expr,
    affix_query: &AffixQuery,
    locator: &Locator,
) -> String {
    let text_str = locator.slice(text);
    let affix_str = locator.slice(affix_query.affix);
    let replacement = affix_query.kind.replacement();
    format!("{text_str}.{replacement}({affix_str})")
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum StmtOrExpr {
    Statement,
    Expression,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum AffixKind {
    StartsWith,
    EndsWith,
}

impl AffixKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::StartsWith => "startswith",
            Self::EndsWith => "endswith",
        }
    }

    const fn replacement(self) -> &'static str {
        match self {
            Self::StartsWith => "removeprefix",
            Self::EndsWith => "removesuffix",
        }
    }
}

/// Components of `startswith(prefix)` or `endswith(suffix)`.
#[derive(Debug)]
struct AffixQuery<'a> {
    /// Whether the method called is `startswith` or `endswith`.
    kind: AffixKind,
    /// Node representing the prefix or suffix being passed to the string method.
    affix: &'a ast::Expr,
}

/// Ingredients for a statement or expression
/// which potentially removes a prefix or suffix from a string.
///
/// Specifically
#[derive(Debug)]
struct RemoveAffixData<'a> {
    /// Node representing the string whose prefix or suffix we want to remove
    text: &'a ast::Expr,
    /// Node representing the bound used to slice the string
    bound: &'a ast::Expr,
    /// Contains the prefix or suffix used in `text.startswith(prefix)` or `text.endswith(suffix)`
    affix_query: AffixQuery<'a>,
}
