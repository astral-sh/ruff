use crate::checkers::ast::Checker;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{self as ast, Decorator, Expr, ExprCall, Keyword, Stmt, StmtFunctionDef};
use ruff_python_semantic::analyze::visibility;
use ruff_python_semantic::{ScopeKind, SemanticModel};
use ruff_python_trivia::PythonWhitespace;
use std::fmt;

pub(super) fn get_mark_decorators(
    decorators: &[Decorator],
) -> impl Iterator<Item = (&Decorator, &str)> {
    decorators.iter().filter_map(|decorator| {
        let name = UnqualifiedName::from_expr(map_callable(&decorator.expression))?;
        let ["pytest", "mark", marker] = name.segments() else {
            return None;
        };
        Some((decorator, *marker))
    })
}

pub(super) fn is_pytest_fail(call: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(call)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "fail"]))
}

pub(super) fn is_pytest_fixture(decorator: &Decorator, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(map_callable(&decorator.expression))
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["pytest", "fixture"]))
}

pub(super) fn is_pytest_yield_fixture(decorator: &Decorator, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(map_callable(&decorator.expression))
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["pytest", "yield_fixture"])
        })
}

pub(super) fn is_pytest_parametrize(call: &ExprCall, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["pytest", "mark", "parametrize"])
        })
}

/// Whether the currently checked `func` is likely to be a Pytest test.
///
/// A normal Pytest test function is one whose name starts with `test` and is either:
///
/// * Placed at module-level, or
/// * Placed within a class whose name starts with `Test` and does not have an `__init__` method.
///
/// During test discovery, Pytest respects a few settings which we do not have access to.
/// This function is thus prone to both false positives and false negatives.
///
/// References:
/// - [`pytest` documentation: Conventions for Python test discovery](https://docs.pytest.org/en/stable/explanation/goodpractices.html#conventions-for-python-test-discovery)
/// - [`pytest` documentation: Changing naming conventions](https://docs.pytest.org/en/stable/example/pythoncollection.html#changing-naming-conventions)
pub(crate) fn is_likely_pytest_test(func: &StmtFunctionDef, checker: &Checker) -> bool {
    let semantic = checker.semantic();

    if !func.name.starts_with("test") {
        return false;
    }

    if semantic.scope_id.is_global() {
        return true;
    }

    let ScopeKind::Class(class) = semantic.current_scope().kind else {
        return false;
    };

    if !class.name.starts_with("Test") {
        return false;
    }

    class.body.iter().all(|stmt| {
        let Stmt::FunctionDef(function) = stmt else {
            return true;
        };

        !visibility::is_init(&function.name)
    })
}

pub(super) fn keyword_is_literal(keyword: &Keyword, literal: &str) -> bool {
    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = &keyword.value {
        value == literal
    } else {
        false
    }
}

pub(super) fn is_empty_or_null_string(expr: &Expr) -> bool {
    match expr {
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.is_empty(),
        Expr::NoneLiteral(_) => true,
        Expr::FString(ast::ExprFString { value, .. }) => {
            value.iter().all(|f_string_part| match f_string_part {
                ast::FStringPart::Literal(literal) => literal.is_empty(),
                ast::FStringPart::FString(f_string) => f_string
                    .elements
                    .iter()
                    .all(is_empty_or_null_fstring_element),
            })
        }
        _ => false,
    }
}

fn is_empty_or_null_fstring_element(element: &ast::FStringElement) -> bool {
    match element {
        ast::FStringElement::Literal(ast::FStringLiteralElement { value, .. }) => value.is_empty(),
        ast::FStringElement::Expression(ast::FStringExpressionElement { expression, .. }) => {
            is_empty_or_null_string(expression)
        }
    }
}

pub(super) fn split_names(names: &str) -> Vec<&str> {
    // Match the following pytest code:
    //    [x.strip() for x in argnames.split(",") if x.strip()]
    names
        .split(',')
        .filter_map(|s| {
            let trimmed = s.trim_whitespace();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        })
        .collect::<Vec<&str>>()
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(super) enum Parentheses {
    None,
    Empty,
}

impl fmt::Display for Parentheses {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Parentheses::None => fmt.write_str(""),
            Parentheses::Empty => fmt.write_str("()"),
        }
    }
}
