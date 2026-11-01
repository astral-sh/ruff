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
    match key.fields() {
        ast::ExprFields::Tuple(ast::ExprTupleFields { node: tuple, .. }) => {
            tuple.iter().all(|elem| is_constant(elem, names))
        }
        ast::ExprFields::Name(ast::ExprNameFields { id, .. }) => !names.contains_key(id.as_str()),
        ast::ExprFields::Attribute(ast::ExprAttributeFields { value, .. }) => {
            is_constant(value, names)
        }
        ast::ExprFields::Subscript(ast::ExprSubscriptFields { value, slice, .. }) => {
            is_constant(value, names) && is_constant(slice, names)
        }
        ast::ExprFields::BinOp(ast::ExprBinOpFields { left, right, .. }) => {
            is_constant(left, names) && is_constant(right, names)
        }
        ast::ExprFields::BoolOp(ast::ExprBoolOpFields { values, .. }) => {
            values.iter().all(|value| is_constant(value, names))
        }
        ast::ExprFields::UnaryOp(ast::ExprUnaryOpFields { operand, .. }) => {
            is_constant(operand, names)
        }
        expr if expr.view().is_literal_expr() => true,
        _ => false,
    }
}
