use crate::Violation;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, StmtFunctionDef};
use ruff_python_parser::parse_expression;
use ruff_text_size::Ranged;
use similar::DiffableStr;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for type annotation which are complex.
///
/// ## Why is this bad?
/// TODO
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.14.9")]
pub(crate) struct ComplexArumentAnnotation {
    argument_name: String,
    complexity_value: isize,
    max_complexity_value: isize,
}

impl Violation for ComplexArumentAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self {
            argument_name,
            complexity_value,
            max_complexity_value,
        } = self;
        format!(
            "Type annotation for argument `{argument_name}` is too complex ({complexity_value} > {max_complexity_value})"
        )
    }
}

fn get_annoation_complexity(expr: &Expr) -> isize {
    if let Some(expr) = expr.as_string_literal_expr() {
        if let Some(literal_value) = expr.as_single_part_string() {
            if let Ok(inner_expr) = parse_expression(&literal_value.value) {
                return get_annoation_complexity(&inner_expr.into_expr());
            }
        }
    };

    if let Some(expr) = expr.as_subscript_expr() {
        let type_params = &expr.slice;

        let inner_compleixty = match &**type_params {
            Expr::Subscript(_) => get_annoation_complexity(type_params),
            Expr::Tuple(expr_tuple) => expr_tuple
                .elts
                .iter()
                .map(|node| get_annoation_complexity(node))
                .max()
                .unwrap_or(0),
            _ => 0,
        };
        return inner_compleixty + 1;
    }

    0
}

// TODO: use config value
const MAX_COMPLEXITY_VALUE: isize = 3;

/// TAE002
pub(crate) fn complex_argument_annotation(checker: &Checker, function_def: &StmtFunctionDef) {
    for arg in function_def.parameters.iter_non_variadic_params() {
        if let Some(type_annotation) = arg.annotation() {
            let annoation_complexity = get_annoation_complexity(type_annotation);
            if annoation_complexity > MAX_COMPLEXITY_VALUE {
                checker.report_diagnostic(
                    ComplexArumentAnnotation {
                        argument_name: arg.name().to_string(),
                        complexity_value: annoation_complexity,
                        max_complexity_value: MAX_COMPLEXITY_VALUE,
                    },
                    type_annotation.range(),
                );
            }
        }
    }
}
