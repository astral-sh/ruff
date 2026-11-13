use rustc_hash::FxHashMap;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::StoredNameFinder;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr, ExprName};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::snippet::SourceCodeSnippet;

/// ## What it does
/// Checks for dictionary comprehensions that use a static key, like a string
/// literal or a variable defined outside the comprehension.
///
/// ## Why is this bad?
/// Using a static key (like a string literal) in a dictionary comprehension
/// is usually a mistake, as it will result in a dictionary with only one key,
/// despite the comprehension iterating over multiple values.
///
/// ## Example
/// ```python
/// data = ["some", "Data"]
/// {"key": value.upper() for value in data}
/// ```
///
/// Use instead:
/// ```python
/// data = ["some", "Data"]
/// {value: value.upper() for value in data}
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.2.0", category = Category::Correctness)]
pub(crate) struct StaticKeyDictComprehension {
    key: SourceCodeSnippet,
}

impl Violation for StaticKeyDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        if let Some(key) = self.key.full_display() {
            format!("Dictionary comprehension uses static key: `{key}`")
        } else {
            "Dictionary comprehension uses static key".to_string()
        }
    }
}

/// B035, RUF011
pub(crate) fn static_key_dict_comprehension(
    checker: &Checker,
    dict_comp: &ast::ExprDictCompFields<'_>,
) {
    let Some(key) = dict_comp.key else {
        return;
    };

    // Collect the bound names in the comprehension's generators.
    let names = {
        let mut visitor = StoredNameFinder::default();
        for generator in dict_comp.generators {
            visitor.visit_comprehension(generator);
        }
        visitor.names
    };

    if is_constant(key, &names) {
        checker.report_diagnostic(
            StaticKeyDictComprehension {
                key: SourceCodeSnippet::from_str(checker.locator().slice(key)),
            },
            key.range(),
        );
    }
}

/// Returns `true` if the given expression is a constant in the context of the dictionary
/// comprehension.
fn is_constant(key: Expr<'_>, names: &FxHashMap<&str, ExprName<'_>>) -> bool {
    match key.classify() {
        ast::ExprKind::Tuple(tuple) => tuple.iter().all(|elem| is_constant(elem, names)),
        ast::ExprKind::Name(name) => !names.contains_key(name.id().as_str()),
        ast::ExprKind::Attribute(attribute) => is_constant(attribute.value(), names),
        ast::ExprKind::Subscript(subscript) => {
            is_constant(subscript.value(), names) && is_constant(subscript.slice(), names)
        }
        ast::ExprKind::BinOp(binary) => {
            is_constant(binary.left(), names) && is_constant(binary.right(), names)
        }
        ast::ExprKind::BoolOp(boolean) => boolean
            .values()
            .iter()
            .all(|value| is_constant(value, names)),
        ast::ExprKind::UnaryOp(unary) => is_constant(unary.operand(), names),
        _ => key.is_literal_expr(),
    }
}
