use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::StoredNameFinder;
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
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
#[violation]
pub struct StaticKeyDictComprehension {
    key: SourceCodeSnippet,
}

impl Violation for StaticKeyDictComprehension {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StaticKeyDictComprehension { key } = self;
        if let Some(key) = key.full_display() {
            format!("Dictionary comprehension uses static key: `{key}`")
        } else {
            format!("Dictionary comprehension uses static key")
        }
    }
}

/// RUF011
pub(crate) fn static_key_dict_comprehension(checker: &mut Checker, dict_comp: &ast::ExprDictComp) {
    // Collect the bound names in the comprehension's generators.
    let names = {
        let mut visitor = StoredNameFinder::default();
        for generator in &dict_comp.generators {
            visitor.visit_comprehension(generator);
        }
        visitor.names
    };

    if is_constant(&dict_comp.key, &names) {
        checker.diagnostics.push(Diagnostic::new(
            StaticKeyDictComprehension {
                key: SourceCodeSnippet::from_str(checker.locator().slice(dict_comp.key.as_ref())),
            },
            dict_comp.key.range(),
        ));
    }
}

/// Returns `true` if the given expression is a constant in the context of the dictionary
/// comprehension.
fn is_constant(key: &Expr, names: &FxHashMap<&str, &ast::ExprName>) -> bool {
    match key {
        Expr::Tuple(tuple) => tuple.iter().all(|elem| is_constant(elem, names)),
        Expr::Name(ast::ExprName { id, .. }) => !names.contains_key(id.as_str()),
        Expr::Attribute(ast::ExprAttribute { value, .. }) => is_constant(value, names),
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            is_constant(value, names) && is_constant(slice, names)
        }
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            is_constant(left, names) && is_constant(right, names)
        }
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
            values.iter().all(|value| is_constant(value, names))
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => is_constant(operand, names),
        expr if expr.is_literal_expr() => true,
        _ => false,
    }
}
