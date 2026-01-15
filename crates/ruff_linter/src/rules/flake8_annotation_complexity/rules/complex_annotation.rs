use crate::Violation;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprBinOp, StmtFunctionDef, name::QualifiedName};
use ruff_python_parser::parse_expression;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type annotation which are complex.
///
/// ## Why is this bad?
/// High type-annotation complexity is a symptom of a complex data structure using generic types.
/// These are hard to comprehend and limit the invariants a type-checker can enforce.
///
/// ## Example
///
/// ```python
/// def example_fn(complex_argument: dict[str, list[dict[str, str | int]]]) -> None: ...
/// ```
///
/// Instead, create concrete data types where possible:
///
/// ```python
/// from dataclasses import dataclass
///
/// @dataclass
/// class Cat:
///     name: str
///     age: int
///
///
/// def example_fn(complex_argument: dict[str, list[Cat]]) -> None: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.14.9")]
pub(crate) struct ComplexAnnotation {
    symbol_name: String,
    complexity_value: isize,
    max_complexity_value: isize,
}

impl Violation for ComplexAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            symbol_name,
            complexity_value,
            max_complexity_value,
        } = self;
        format!(
            "Type annotation for `{symbol_name}` is too complex ({complexity_value} > {max_complexity_value})"
        )
    }
}

trait AnnotationResolver {
    fn resolve_annoation_qualified_name<'a, 'expr>(
        &'a self,
        expr: &'expr Expr,
    ) -> Option<QualifiedName<'expr>>
    where
        'a: 'expr;

    /// Resolve if a given Expr refers to `Annotated` from the `typing` or `typing_extensions`
    /// module
    fn is_typing_annotated(&self, expr: &Expr) -> bool {
        self.resolve_annoation_qualified_name(expr)
            .map(|qualified_name| match qualified_name.segments() {
                ["typing", "Annotated"] => true,
                ["typing_extensions", "Annotated"] => true,
                _ => false,
            })
            .unwrap_or(false)
    }
}

struct CheckerAnnoationResolver<'a, 'b>
where
    'b: 'a,
{
    checker: &'a Checker<'b>,
}

impl<'checker, 'b> AnnotationResolver for CheckerAnnoationResolver<'checker, 'b> {
    fn resolve_annoation_qualified_name<'a, 'expr>(
        &'a self,
        expr: &'expr Expr,
    ) -> Option<QualifiedName<'expr>>
    where
        'a: 'expr,
    {
        self.checker.semantic().resolve_qualified_name(expr)
    }
}

/// Flattens a left-associative BinOp list into an iterator a vector of non-bin-op expressions.
/// This is useful for flattening PEP-604 Unions into a list of types.
fn flatten_bin_op_expr<'a>(expr: &'a ExprBinOp) -> Vec<&'a Expr> {
    let mut looking_at = expr;
    let mut result: Vec<&Expr> = vec![&looking_at.right];

    while let Some(expr) = looking_at.left.as_bin_op_expr() {
        looking_at = expr;
        result.push(&expr.right);
    }
    result.push(&looking_at.left);

    result
}

fn get_annoation_complexity<'checker, 'expr>(
    annoation_resolver: &'checker impl AnnotationResolver,
    expr: &'expr Expr,
) -> isize
where
    'checker: 'expr,
{
    if let Some(expr) = expr.as_string_literal_expr() {
        if let Some(literal_value) = expr.as_single_part_string() {
            if let Ok(inner_expr) = parse_expression(&literal_value.value) {
                return get_annoation_complexity(annoation_resolver, &inner_expr.into_expr());
            }
        }
    };

    if let Some(expr_bin_op) = expr.as_bin_op_expr() {
        return flatten_bin_op_expr(expr_bin_op)
            .iter()
            .map(|node| get_annoation_complexity(annoation_resolver, node))
            .max()
            .unwrap_or(0)
            + 1;
    }

    if let Some(expr) = expr.as_subscript_expr() {
        let type_params = &expr.slice;

        let inner_compleixty = match &**type_params {
            Expr::Subscript(_) | Expr::BinOp(_) => {
                get_annoation_complexity(annoation_resolver, type_params)
            }
            Expr::Tuple(expr_tuple) => expr_tuple
                .elts
                .iter()
                .map(|node| get_annoation_complexity(annoation_resolver, node))
                .max()
                .unwrap_or(0),
            _ => 0,
        };
        if annoation_resolver.is_typing_annotated(&expr.value) {
            return inner_compleixty;
        }
        return inner_compleixty + 1;
    }

    0
}

/// TAE002
pub(crate) fn complex_annotation(checker: &Checker, function_def: &StmtFunctionDef) {
    let max_complexity = checker
        .settings()
        .flake8_annotation_complexity
        .max_annotation_complexity;

    let annoation_resolver = CheckerAnnoationResolver { checker };

    for arg in function_def.parameters.iter_non_variadic_params() {
        if let Some(type_annotation) = arg.annotation() {
            let annoation_complexity =
                get_annoation_complexity(&annoation_resolver, type_annotation);
            if annoation_complexity > max_complexity {
                checker.report_diagnostic(
                    ComplexAnnotation {
                        symbol_name: arg.name().to_string(),
                        complexity_value: annoation_complexity,
                        max_complexity_value: max_complexity,
                    },
                    type_annotation.range(),
                );
            }
        }
    }

    if let Some(return_annotation) = &function_def.returns {
        let annoation_complexity =
            get_annoation_complexity(&annoation_resolver, &return_annotation);
        if annoation_complexity > max_complexity {
            checker.report_diagnostic(
                ComplexAnnotation {
                    symbol_name: "return type".to_owned(),
                    complexity_value: annoation_complexity,
                    max_complexity_value: max_complexity,
                },
                return_annotation.range(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_expression;
    use test_case::test_case;

    struct FromTypingResolver;

    impl AnnotationResolver for FromTypingResolver {
        fn resolve_annoation_qualified_name<'a, 'expr>(
            &'a self,
            expr: &'expr Expr,
        ) -> Option<QualifiedName<'expr>>
        where
            'a: 'expr,
        {
            if let Some(name) = expr.as_name_expr() {
                match name.id.as_str() {
                    "Annotated" => Some(QualifiedName::from_dotted_name("typing.Annotated")),
                    _ => None,
                }
            } else {
                None
            }
        }
    }

    #[test_case(r"int", 0)]
    #[test_case(r"dict[str, Any]", 1)]
    #[test_case(r"dict[str, list[dict[str, str]]]", 3)]
    #[test_case(r"dict[str, int | str | bool]", 2)]
    #[test_case(r"dict[str, Union[int, str, bool]]", 2)]
    #[test_case(r#""dict[str, list[list]]""#, 2)]
    #[test_case(r#"dict[str, "list[str]"]"#, 2)]
    #[test_case(r#"Union[a, b, c]"#, 1)]
    #[test_case(r#"a | b | c"#, 1)]
    #[test_case(r#"a | dict[str, str] | c"#, 2)]
    #[test_case(r#"a | dict[str, str] | dict[str, str]"#, 2)]
    #[test_case(r#"a | dict[str, str] | dict[str, list[str]]"#, 3)]
    #[test_case(r#"list[Union[a, b, c]]"#, 2)]
    #[test_case(r#"list[a | b | c]"#, 2)]
    fn test_get_annoation_complexity_yields_expected_value(
        annotation: &str,
        expected_complexity: isize,
    ) {
        let expr = parse_expression(annotation).unwrap();
        let complexity = get_annoation_complexity(&FromTypingResolver {}, &expr.expr());
        assert_eq!(complexity, expected_complexity);
    }

    #[test]
    fn test_flattern_bin_op_expr() {
        let expr = parse_expression("hello | there | my | friend").unwrap();
        let flatterned: Vec<_> = flatten_bin_op_expr(&expr.expr().as_bin_op_expr().unwrap())
            .iter()
            .flat_map(|node| node.as_name_expr().map(|x| x.id.as_str()))
            .collect();

        // Note: left-associativity reverses the expected liet
        let expected = vec!["friend", "my", "there", "hello"];
        assert_eq!(flatterned, expected);
    }
}
