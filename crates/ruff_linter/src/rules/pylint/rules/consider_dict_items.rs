use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usages where
/// a value is extracted through the key of dictionary in an iteration, when it simply be extracted using `.items()`
///
/// ## Why is this bad?
/// Instead of unnecsarily indexing the the dictionary, it's more semantically clear to extract the value
/// one-per-one with the key, increasing readability.
///
///
/// ## Example
/// ```python
/// ORCHESTRA = {
///     "violin": "strings",
///     "oboe": "woodwind",
///     "tuba": "brass",
///     "gong": "percussion",
/// }
///
/// for instrument in ORCHESTRA:
///     print(f"{instrument}: {ORCHESTRA[instrument]}")
/// ```
///
/// Use instead:
/// ```python
/// ORCHESTRA = {
///     "violin": "strings",
///     "oboe": "woodwind",
///     "tuba": "brass",
///     "gong": "percussion",
/// }
///
/// for instrument, section in ORCHESTRA.items():
///     print(f"{instrument}: {section}")
/// ```

#[violation]
pub struct ConsiderDictItems;

impl Violation for ConsiderDictItems {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Extracting value from dictionary in iteration without calling `.items()`")
    }
}

/// PLC0206
pub(crate) fn consider_dict_items(checker: &mut Checker, stmt_for: &ast::StmtFor) {
    let ast::StmtFor {
        target,
        iter,
        body,
        orelse: _,
        is_async: _,
        range: _,
    } = stmt_for;

    let Some(iter_obj_name) = iter.as_name_expr() else {
        return;
    };

    for child in body {
        // If it isn't an expression, skip it.
        let Some(child_expr) = child.as_expr_stmt() else {
            continue;
        };
        let expr_value = &child_expr.value;
        if let ast::Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _,
        }) = &**expr_value
        {
            if !value.is_name_expr() && !value.is_attribute_expr() {
                return;
            }

            // Check that the sliced value is the same as the target of the for loop.
            let slice_name = slice.as_name_expr();
            let target_name = target.as_name_expr();

            if slice_name.is_none() || target_name.is_none() {
                return;
            }

            let slice_name = slice_name.unwrap();
            let target_name = target_name.unwrap();

            if slice_name.id != target_name.id {
                return;
            }

            // Check that the sliced dict name is the same as the iterated object name.
            if !(slice
                .as_name_expr()
                .is_some_and(|name| name.id == iter_obj_name.id))
            {
                return;
            }

            let diagnostic = Diagnostic::new(ConsiderDictItems, stmt_for.range);
            checker.diagnostics.push(diagnostic);
        } else {
            return;
        }
    }
}
