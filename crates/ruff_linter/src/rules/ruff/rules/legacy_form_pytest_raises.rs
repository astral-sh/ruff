use itertools::{Either, Itertools};
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Stmt, StmtExpr, StmtWith, WithItem};
use ruff_python_semantic::SemanticModel;
use ruff_python_trivia::{has_leading_content, has_trailing_content, leading_indentation};
use ruff_source_file::UniversalNewlines;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for non-contextmanager use of `pytest.raises`.
///
/// ## Why is this bad?
/// The context-manager form is more readable, easier to extend, and supports additional kwargs.
///
/// ## Example
/// ```python
/// import pytest
///
///
/// excinfo = pytest.raises(ValueError, int, "hello")
/// ```
///
/// Use instead:
/// ```python
/// import pytest
///
///
/// with pytest.raises(ValueError) as excinfo:
///     int("hello")
/// ```
///
/// ## References
/// - [`pytest` documentation: `pytest.raises`](https://docs.pytest.org/en/latest/reference/reference.html#pytest-raises)
#[derive(ViolationMetadata)]
pub(crate) struct LegacyFormPytestRaises;

impl Violation for LegacyFormPytestRaises {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Use context-manager form of `pytest.raises()`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Use `pytest.raises()` as a context-manager".to_string())
    }
}

pub(crate) fn is_pytest_raises(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "raises"]))
}

pub(crate) fn legacy_raises_call(checker: &Checker, call: &ast::ExprCall) {
    if is_pytest_raises(&call.func, checker.semantic()) {
        if checker.enabled(Rule::LegacyFormPytestRaises)
            && call.arguments.find_argument("func", 1).is_some()
        {
            let mut diagnostic = Diagnostic::new(LegacyFormPytestRaises, call.range());
            let stmt = checker.semantic().current_statement();
            if !has_leading_content(stmt.start(), checker.source())
                && !has_trailing_content(stmt.end(), checker.source())
            {
                if let Some(with_stmt) = try_fix_legacy_raises(stmt, checker.semantic()) {
                    let generated = checker.generator().stmt(&Stmt::With(with_stmt));
                    let first_line = checker.locator().line_str(stmt.start());
                    let indentation = leading_indentation(first_line);
                    let mut indented = String::new();
                    for (idx, line) in generated.universal_newlines().enumerate() {
                        if idx == 0 {
                            indented.push_str(&line);
                        } else {
                            indented.push_str(checker.stylist().line_ending().as_str());
                            indented.push_str(indentation);
                            indented.push_str(&line);
                        }
                    }

                    diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                        indented,
                        stmt.range(),
                    )));
                }
            }
            checker.report_diagnostic(diagnostic);
        }
    }
}

fn try_fix_legacy_raises(stmt: &Stmt, semantic: &SemanticModel) -> Option<StmtWith> {
    match stmt {
        Stmt::Expr(StmtExpr { value, .. }) => {
            let call = value.as_call_expr()?;

            if is_pytest_raises(&call.func, semantic) {
                generate_with_raises(call, None, None)
            } else {
                let inner_raises_call = call
                    .func
                    .as_attribute_expr()
                    .filter(|expr_attribute| &expr_attribute.attr == "match")
                    .and_then(|expr_attribute| expr_attribute.value.as_call_expr())
                    .filter(|inner_call| is_pytest_raises(&inner_call.func, semantic))?;
                generate_with_raises(inner_raises_call, call.arguments.args.first(), None)
            }
        }
        Stmt::Assign(ast::StmtAssign {
            range: _,
            targets,
            value,
        }) => {
            let [target] = targets.as_slice() else {
                return None;
            };

            let raises_call = value
                .as_call_expr()
                .filter(|call| is_pytest_raises(&call.func, semantic))?;

            let optional_vars = Some(target);
            let match_call = None;
            generate_with_raises(raises_call, match_call, optional_vars)
        }
        _ => None,
    }
}

fn generate_with_raises(
    legacy_raises_call: &ast::ExprCall,
    match_arg: Option<&Expr>,
    optional_vars: Option<&Expr>,
) -> Option<StmtWith> {
    let expected_exception = legacy_raises_call
        .arguments
        .find_argument_value("expected_exception", 0)?;

    let func = legacy_raises_call
        .arguments
        .find_argument_value("func", 1)?;

    let raises_call = ast::ExprCall {
        range: TextRange::default(),
        func: legacy_raises_call.func.clone(),
        arguments: ast::Arguments {
            range: TextRange::default(),
            args: Box::new([expected_exception.clone()]),
            keywords: match_arg
                .map(|expr| ast::Keyword {
                    // Take range from the original expression so that the keyword
                    // argument is generated after positional arguments
                    range: expr.range(),
                    arg: Some(ast::Identifier::new("match", TextRange::default())),
                    value: expr.clone(),
                })
                .as_slice()
                .into(),
        },
    };

    let (func_args, func_keywords): (Vec<_>, Vec<_>) = legacy_raises_call
        .arguments
        .arguments_source_order()
        .skip(2)
        .partition_map(|arg_or_keyword| match arg_or_keyword {
            ast::ArgOrKeyword::Arg(expr) => Either::Left(expr.clone()),
            ast::ArgOrKeyword::Keyword(keyword) => Either::Right(keyword.clone()),
        });
    let func_args = func_args.into_boxed_slice();
    let func_keywords = func_keywords.into_boxed_slice();

    let func_call = ast::ExprCall {
        range: TextRange::default(),
        func: Box::new(func.clone()),
        arguments: ast::Arguments {
            range: TextRange::default(),
            args: func_args,
            keywords: func_keywords,
        },
    };

    Some(StmtWith {
        range: TextRange::default(),
        is_async: false,
        items: vec![WithItem {
            range: TextRange::default(),
            context_expr: raises_call.into(),
            optional_vars: optional_vars.map(|var| Box::new(var.clone())),
        }],
        body: vec![Stmt::Expr(StmtExpr {
            range: TextRange::default(),
            value: Box::new(func_call.into()),
        })],
    })
}
