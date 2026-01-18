use crate::Violation;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprBinOp, StmtAnnAssign, StmtFunctionDef};
use ruff_python_parser::parse_expression;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type annotation which are too complex.
///
/// ## Why is this bad?
/// Annotation complexity is a symptom of using generic types for complex, nested data structures.
/// These are hard to comprehend and limit the invariants a type-checker can enforce.
///
/// ## Example
///
/// ```python
/// def example_fn(complex_argument: dict[str, list[dict[str, str]]]) -> None: ...
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
///     color: str
///
///
/// def example_fn(complex_argument: dict[str, list[Cat]]) -> None: ...
/// ```
///
/// Or consider user a type alias:
///
/// ```python
///
/// TaskUserMap = dict[str, str]
///
/// def example_fn(complex_argument: dict[str, list[TaskUserMap]]) -> None: ...
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.14.14")]
pub(crate) struct ComplexAnnotation {
    symbol_name: String,
    complexity_value: usize,
    max_complexity_value: usize,
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

    fn fix_title(&self) -> Option<String> {
        Some("consider using a type alias or refactoring into a `dataclass`".to_string())
    }
}

/// TAE002 (on function)
pub(crate) fn complex_annotation_function(checker: &Checker, function_def: &StmtFunctionDef) {
    let max_complexity = checker
        .settings()
        .flake8_annotation_complexity
        .max_annotation_complexity;

    let annotation_resolver = CheckerAnnotationResolver { checker };

    for arg in function_def.parameters.iter_non_variadic_params() {
        if let Some(type_annotation) = arg.annotation() {
            let annotation_complexity =
                get_annotation_complexity(&annotation_resolver, type_annotation);
            if annotation_complexity > max_complexity {
                checker.report_diagnostic(
                    ComplexAnnotation {
                        symbol_name: arg.name().to_string(),
                        complexity_value: annotation_complexity,
                        max_complexity_value: max_complexity,
                    },
                    type_annotation.range(),
                );
            }
        }
    }

    if let Some(return_annotation) = &function_def.returns {
        let annotation_complexity =
            get_annotation_complexity(&annotation_resolver, &return_annotation);
        if annotation_complexity > max_complexity {
            checker.report_diagnostic(
                ComplexAnnotation {
                    symbol_name: "return type".to_owned(),
                    complexity_value: annotation_complexity,
                    max_complexity_value: max_complexity,
                },
                return_annotation.range(),
            );
        }
    }
}

/// TAE002 (on assignment)
pub(crate) fn complex_annotation_assignment(checker: &Checker, assign_stmt: &StmtAnnAssign) {
    let max_complexity = checker
        .settings()
        .flake8_annotation_complexity
        .max_annotation_complexity;

    let annotation_resolver = CheckerAnnotationResolver { checker };

    let annotation_complexity =
        get_annotation_complexity(&annotation_resolver, &assign_stmt.annotation);
    if annotation_complexity > max_complexity {
        checker.report_diagnostic(
            ComplexAnnotation {
                symbol_name: assign_stmt
                    .target
                    .as_name_expr()
                    .map(|x| x.id.as_str())
                    .unwrap_or("")
                    .to_owned(),
                complexity_value: annotation_complexity,
                max_complexity_value: max_complexity,
            },
            assign_stmt.annotation.range(),
        );
    }
}

/// Indirection of `checker.semantic.resolve_qualified_name` to allow unit-testing of
/// annotation complexity calculation on a single annotation expression.
trait AnnotationResolver {
    /// Returns if a Expr references `typing.Annotated`
    fn is_annotated_type(&self, expr: &Expr) -> bool;
}

struct CheckerAnnotationResolver<'a, 'b>
where
    'b: 'a,
{
    checker: &'a Checker<'b>,
}

impl<'checker, 'b> AnnotationResolver for CheckerAnnotationResolver<'checker, 'b> {
    fn is_annotated_type(&self, expr: &Expr) -> bool {
        self.checker
            .semantic()
            .resolve_qualified_name(expr)
            .map(|qualified_name| {
                self.checker
                    .semantic()
                    .match_typing_qualified_name(&qualified_name, "Annotated")
            })
            .unwrap_or(false)
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

fn get_annotation_complexity<'checker, 'expr>(
    annotation_resolver: &'checker impl AnnotationResolver,
    expr: &'expr Expr,
) -> usize
where
    'checker: 'expr,
{
    if let Some(expr) = expr.as_string_literal_expr()
        && let Some(literal_value) = expr.as_single_part_string()
        && let Ok(inner_expr) = parse_expression(&literal_value.value)
    {
        return get_annotation_complexity(annotation_resolver, &inner_expr.into_expr());
    };

    if let Some(expr_bin_op) = expr.as_bin_op_expr() {
        return flatten_bin_op_expr(expr_bin_op)
            .iter()
            .map(|node| get_annotation_complexity(annotation_resolver, node))
            .max()
            .unwrap_or(0)
            + 1;
    }

    if let Some(expr) = expr.as_subscript_expr() {
        let type_params = &expr.slice;

        let inner_compleixty = match &**type_params {
            Expr::Subscript(_) | Expr::BinOp(_) => {
                get_annotation_complexity(annotation_resolver, type_params)
            }
            Expr::Tuple(expr_tuple) => expr_tuple
                .elts
                .iter()
                .map(|node| get_annotation_complexity(annotation_resolver, node))
                .max()
                .unwrap_or(0),
            _ => 0,
        };
        if annotation_resolver.is_annotated_type(&expr.value) {
            return inner_compleixty;
        }
        return inner_compleixty + 1;
    }

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruff_python_parser::parse_expression;
    use test_case::test_case;

    struct FromTypingResolver;

    impl AnnotationResolver for FromTypingResolver {
        fn is_annotated_type(&self, expr: &Expr) -> bool {
            if let Some(name) = expr.as_name_expr() {
                name.id.as_str() == "Annotated"
            } else {
                false
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
    fn test_get_annotation_complexity_yields_expected_value(
        annotation: &str,
        expected_complexity: usize,
    ) {
        let expr = parse_expression(annotation).unwrap();
        let complexity = get_annotation_complexity(&FromTypingResolver {}, &expr.expr());
        assert_eq!(complexity, expected_complexity);
    }

    #[test]
    fn test_flatten_bin_op_expr() {
        let expr = parse_expression("hello | there | my | friend").unwrap();
        let flattened: Vec<_> = flatten_bin_op_expr(&expr.expr().as_bin_op_expr().unwrap())
            .iter()
            .flat_map(|node| node.as_name_expr().map(|x| x.id.as_str()))
            .collect();

        // Note: left-associativity reverses the expected liet
        let expected = vec!["friend", "my", "there", "hello"];
        assert_eq!(flattened, expected);
    }
}
