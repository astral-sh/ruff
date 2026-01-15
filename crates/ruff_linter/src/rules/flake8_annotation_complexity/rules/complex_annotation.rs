use crate::Violation;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, StmtFunctionDef, name::QualifiedName};
use ruff_python_parser::parse_expression;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type annotation which are complex.
///
/// ## Why is this bad?
/// TODO
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

    /// Resolve if a given Expr reffers to `Annotated` from the `typing` or `typing_extensions`
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

    if let Some(expr) = expr.as_subscript_expr() {
        let type_params = &expr.slice;

        let inner_compleixty = match &**type_params {
            Expr::Subscript(_) => get_annoation_complexity(annoation_resolver, type_params),
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
    #[test_case(r"dict[str, int | str | bool]", 1)]
    #[test_case(r"dict[str, Union[int, str, bool]]", 2)]
    #[test_case(r#""dict[str, list[list]]""#, 2)]
    #[test_case(r#"dict[str, "list[str]"]"#, 2)]
    fn get_annoation_complexity_yields_expected_value(
        annotation: &str,
        expected_complexity: isize,
    ) {
        let expr = parse_expression(annotation).unwrap();
        let complexity = get_annoation_complexity(&FromTypingResolver {}, &expr.expr());
        assert_eq!(complexity, expected_complexity);
    }
}
