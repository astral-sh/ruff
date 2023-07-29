use anyhow::Result;
use ruff_python_ast::{self as ast, CmpOp, Expr, Ranged};
use ruff_text_size::TextRange;

use ruff_diagnostics::Edit;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_codegen::Stylist;
use ruff_source_file::Locator;

use crate::autofix::codemods::CodegenStylist;
use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_attribute, match_call_mut, match_expression};
use crate::registry::AsRule;

/// ## What it does
/// Checks for key-existence checks against `dict.keys()` calls.
///
/// ## Why is this bad?
/// When checking for the existence of a key in a given dictionary, using
/// `key in dict` is more readable and efficient than `key in dict.keys()`,
/// while having the same semantics.
///
/// ## Example
/// ```python
/// key in foo.keys()
/// ```
///
/// Use instead:
/// ```python
/// key in foo
/// ```
///
/// ## References
/// - [Python documentation: Mapping Types](https://docs.python.org/3/library/stdtypes.html#mapping-types-dict)
#[violation]
pub struct InDictKeys {
    key: String,
    dict: String,
    operator: String,
}

impl AlwaysAutofixableViolation for InDictKeys {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InDictKeys {
            key,
            dict,
            operator,
        } = self;
        format!("Use `{key} {operator} {dict}` instead of `{key} {operator} {dict}.keys()`")
    }

    fn autofix_title(&self) -> String {
        let InDictKeys {
            key,
            dict,
            operator,
        } = self;
        format!("Convert to `{key} {operator} {dict}`")
    }
}

/// SIM118
fn key_in_dict(
    checker: &mut Checker,
    left: &Expr,
    right: &Expr,
    operator: CmpOp,
    range: TextRange,
) {
    let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        range: _,
    }) = &right
    else {
        return;
    };
    if !(args.is_empty() && keywords.is_empty()) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = func.as_ref() else {
        return;
    };
    if attr != "keys" {
        return;
    }

    // Ignore `self.keys()`, which will almost certainly be intentional, as in:
    // ```python
    // def __contains__(self, key: object) -> bool:
    //     return key in self.keys()
    // ```
    if value
        .as_name_expr()
        .map_or(false, |name| matches!(name.id.as_str(), "self"))
    {
        return;
    }

    // Slice exact content to preserve formatting.
    let left_content = checker.locator().slice(left.range());
    let Ok(value_content) =
        value_content_for_key_in_dict(checker.locator(), checker.stylist(), right)
    else {
        return;
    };

    let mut diagnostic = Diagnostic::new(
        InDictKeys {
            key: left_content.to_string(),
            dict: value_content.to_string(),
            operator: operator.as_str().to_string(),
        },
        range,
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            value_content,
            right.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}

/// SIM118 in a for loop
pub(crate) fn key_in_dict_for(checker: &mut Checker, target: &Expr, iter: &Expr) {
    key_in_dict(
        checker,
        target,
        iter,
        CmpOp::In,
        TextRange::new(target.start(), iter.end()),
    );
}

/// SIM118 in a comparison
pub(crate) fn key_in_dict_compare(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
) {
    let [op] = ops else {
        return;
    };

    if !matches!(op, CmpOp::In | CmpOp::NotIn) {
        return;
    }

    let [right] = comparators else {
        return;
    };

    key_in_dict(checker, left, right, *op, expr.range());
}

fn value_content_for_key_in_dict(
    locator: &Locator,
    stylist: &Stylist,
    expr: &Expr,
) -> Result<String> {
    let content = locator.slice(expr.range());
    let mut expression = match_expression(content)?;
    let call = match_call_mut(&mut expression)?;
    let attribute = match_attribute(&mut call.func)?;
    Ok(attribute.value.codegen_stylist(stylist))
}
