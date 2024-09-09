use crate::{checkers::ast::Checker, settings::types::PythonVersion};
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

/// ## What it does
/// Checks for the removal of a prefix or suffix from a string by assigning
/// the string to a slice after checking `startswith` or `endswith`, respectively.
///
/// ## Why is this bad?
/// The methods `removeprefix` and `removesuffix`,
/// introduced in Python 3.9, have the same behavior
/// and are more readable and efficient.
///
/// ## Example
/// ```python
/// filename[:-4] if filename.endswith(".txt") else filename
/// ```
///
/// ```python
/// if text.startswith("pre"):
///     text = text[3:]
/// ```
///
/// Use instead:
/// ```python
/// filename = filename.removesuffix(".txt")
/// ```
///
/// ```python
/// text = text.removeprefix("pre")
/// ```
#[violation]
pub struct SliceToRemovePrefixOrSuffix {
    string: String,
    affix_kind: AffixKind,
    stmt_or_expression: StmtOrExpr,
}

impl AlwaysFixableViolation for SliceToRemovePrefixOrSuffix {
    #[derive_message_formats]
    fn message(&self) -> String {
        match self.affix_kind {
            AffixKind::StartsWith => {
                format!("Prefer `removeprefix` over conditionally replacing with slice.")
            }
            AffixKind::EndsWith => {
                format!("Prefer `removesuffix` over conditionally replacing with slice.")
            }
        }
    }

    fn fix_title(&self) -> String {
        let (to_replace, replacement) = match self.affix_kind {
            AffixKind::StartsWith => ("`startswith`", "`removeprefix`"),
            AffixKind::EndsWith => ("`endswith`", "`removesuffix`"),
        };
        let context = match self.stmt_or_expression {
            StmtOrExpr::Statement => "assignment",
            StmtOrExpr::Expression => "ternary expression",
        };
        format!("Use {replacement} instead of {context} conditional upon {to_replace}.")
    }
}

/// FURB188
pub(crate) fn slice_to_remove_affix_expr(checker: &mut Checker, if_expr: &ast::ExprIf) {
    if checker.settings.target_version < PythonVersion::Py39 {
        return;
    }

    if let Some(removal_data) = affix_removal_data_expr(if_expr) {
        if affix_matches_slice_bound(&removal_data, checker) {
            let kind = removal_data.affix_query.kind;
            let text = removal_data.text;

            let mut diagnostic = Diagnostic::new(
                SliceToRemovePrefixOrSuffix {
                    affix_kind: kind,
                    string: text.to_string(),
                    stmt_or_expression: StmtOrExpr::Expression,
                },
                if_expr.range,
            );
            let replacement =
                generate_removeaffix_expr(text, &removal_data.affix_query, checker.generator());

            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                replacement,
                if_expr.start(),
                if_expr.end(),
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}

/// FURB188
pub(crate) fn slice_to_remove_affix_stmt(checker: &mut Checker, if_stmt: &ast::StmtIf) {
    if checker.settings.target_version < PythonVersion::Py39 {
        return;
    }
    if let Some(removal_data) = affix_removal_data_stmt(if_stmt) {
        if affix_matches_slice_bound(&removal_data, checker) {
            let kind = removal_data.affix_query.kind;
            let text = removal_data.text;

            let mut diagnostic = Diagnostic::new(
                SliceToRemovePrefixOrSuffix {
                    affix_kind: kind,
                    string: text.to_string(),
                    stmt_or_expression: StmtOrExpr::Statement,
                },
                if_stmt.range,
            );

            let replacement = generate_assignment_with_removeaffix(
                text,
                &removal_data.affix_query,
                checker.generator(),
            );

            diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
                replacement,
                if_stmt.start(),
                if_stmt.end(),
            )));
            checker.diagnostics.push(diagnostic);
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
    let else_or_target_name = &orelse.as_name_expr()?.id;
    // Variable names correspond to:
    // ```python
    // value[slice] if test else else_or_target_name
    // ```
    affix_removal_data(value, test, else_or_target_name, slice)
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
    };

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
    let else_or_target_name = &target.as_name_expr()?.id;
    let ast::ExprSubscript { value, slice, .. } = value.as_subscript_expr()?;

    affix_removal_data(value, test, else_or_target_name, slice)
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
///     - `value` and `else_or_target_name`
/// are equal to a common name `text`
///     - `test` is of the form `text.startswith(prefix)`
/// or `text.endswith(suffix)`
///     - `slice` has no upper bound in the case of a prefix,
/// and no lower bound in the case of a suffix
///
/// If these conditions are satisfied, the function
/// returns the corresponding `RemoveAffixData` object;
/// otherwise it returns `None`.
fn affix_removal_data<'a>(
    value: &'a ast::Expr,
    test: &'a ast::Expr,
    else_or_target_name: &'a ast::name::Name,
    slice: &'a ast::Expr,
) -> Option<RemoveAffixData<'a>> {
    let body_name = &value.as_name_expr()?.id;
    let slice = slice.as_slice_expr()?;
    let test_name = &test
        .as_call_expr()?
        .func
        .as_attribute_expr()?
        .value
        .as_name_expr()?
        .id;
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
    if body_name != test_name || test_name != else_or_target_name {
        return None;
    }
    let (affix_kind, bound) = match func_name {
        "startswith" if slice.upper.is_none() => (AffixKind::StartsWith, slice.lower.as_ref()?),
        "endswith" if slice.lower.is_none() => (AffixKind::EndsWith, slice.upper.as_ref()?),
        _ => return None,
    };
    Some(RemoveAffixData {
        text: body_name,
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
/// this function verifies that `bound == -len(suffix)` in two cases:
///     - `suffix` is a string literal and `bound` is a number literal
///     - `suffix` is an expression and `bound` is
///     exactly `-len(suffix)` (as AST nodes, prior to evaluation.)
fn affix_matches_slice_bound(data: &RemoveAffixData, checker: &mut Checker) -> bool {
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
        ) => num.as_int().is_some_and(|x| {
            // Only support prefix removal for size at most `u32::MAX`
            u32::try_from(string_val.len()).is_ok_and(|length| x == &ast::Int::from(length))
        }),
        (
            AffixKind::StartsWith,
            ast::Expr::Call(ast::ExprCall {
                range: _,
                func,
                arguments,
            }),
            _,
        ) => {
            checker.semantic().match_builtin_expr(func, "len")
                && arguments.len() == 1
                && arguments.find_positional(0).is_some_and(|arg| {
                    let compr_affix = ast::comparable::ComparableExpr::from(affix);
                    let compr_arg = ast::comparable::ComparableExpr::from(arg);
                    compr_affix == compr_arg
                })
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
        ) => operand.as_number_literal_expr().is_some_and(
            |ast::ExprNumberLiteral { value, .. }| {
                value.as_int().is_some_and(|x| {
                    // Only support prefix removal for size at most `u32::MAX`
                    u32::try_from(string_val.len()).is_ok_and(|length| x == &ast::Int::from(length))
                })
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
                func.as_name_expr()
                    .is_some_and(|name| name.id == ast::name::Name::new("len"))
                    && arguments.len() == 1
                    && arguments.find_positional(0).is_some_and(|arg| {
                        let compr_affix = ast::comparable::ComparableExpr::from(affix);
                        let compr_arg = ast::comparable::ComparableExpr::from(arg);
                        compr_affix == compr_arg
                    })
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
    text: &str,
    affix_query: &AffixQuery,
    generator: Generator,
) -> String {
    let remove_affix_expr = make_removeaffix_expr(text, affix_query);
    generator.stmt(&ast::Stmt::Assign(ast::StmtAssign {
        range: TextRange::default(),
        targets: vec![ast::Expr::Name(ast::ExprName {
            range: TextRange::default(),
            id: ast::name::Name::from(text),
            ctx: ast::ExprContext::Store,
        })],
        value: Box::new(remove_affix_expr),
    }))
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
fn generate_removeaffix_expr(text: &str, affix_query: &AffixQuery, generator: Generator) -> String {
    let remove_affix_expr = make_removeaffix_expr(text, affix_query);
    generator.expr(&remove_affix_expr)
}

/// Creates the AST node corresponding to
/// ```python
/// text.removeprefix(prefix)
/// ```
/// or
///
/// ```python
/// text.removesuffix(suffix)
/// ```
/// as appropriate.
fn make_removeaffix_expr(text: &str, affix_query: &AffixQuery) -> ast::Expr {
    let func_name = match affix_query.kind {
        AffixKind::StartsWith => ast::name::Name::from("removeprefix"),
        AffixKind::EndsWith => ast::name::Name::from("removesuffix"),
    };
    ast::Expr::Call(ast::ExprCall {
        range: TextRange::default(),
        func: Box::new(ast::Expr::Attribute(ast::ExprAttribute {
            range: TextRange::default(),
            value: Box::new(ast::Expr::Name(ast::ExprName {
                range: TextRange::default(),
                id: ast::name::Name::from(text),
                ctx: ast::ExprContext::Store,
            })),
            attr: ast::Identifier {
                id: func_name,
                range: TextRange::default(),
            },
            ctx: ast::ExprContext::Load,
        })),
        arguments: ast::Arguments {
            range: TextRange::default(),
            args: Box::new([affix_query.affix.clone()]),
            keywords: Box::new([]),
        },
    })
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

/// Components of `startswith(prefix)` or `endswith(suffix)`.
#[derive(Debug)]
struct AffixQuery<'a> {
    /// Whether the method called is `startswith` or `endswith`.
    kind: AffixKind,
    /// The prefix or suffix being passed to the string method.
    affix: &'a ast::Expr,
}

/// Ingredients for a statement or expression
/// which potentially removes a prefix or suffix from a string.
///
/// Specifically
#[derive(Debug)]
struct RemoveAffixData<'a> {
    /// The string whose prefix or suffix we want to remove
    text: &'a str,
    /// Bound used to slice the string
    bound: &'a ast::Expr,
    /// Contains the prefix or suffix used in `text.startswith(prefix)` or `text.endswith(suffix)`
    affix_query: AffixQuery<'a>,
}
