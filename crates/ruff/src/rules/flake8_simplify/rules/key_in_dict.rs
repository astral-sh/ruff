use anyhow::Result;
use libcst_native::{Codegen, CodegenState};
use log::error;
use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Cmpop, Expr, Ranged};

use ruff_diagnostics::Edit;
use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::{Locator, Stylist};

use crate::checkers::ast::Checker;
use crate::cst::matchers::{match_attribute, match_call_mut, match_expression};
use crate::registry::AsRule;

#[violation]
pub struct InDictKeys {
    key: String,
    dict: String,
}

impl AlwaysAutofixableViolation for InDictKeys {
    #[derive_message_formats]
    fn message(&self) -> String {
        let InDictKeys { key, dict } = self;
        format!("Use `{key} in {dict}` instead of `{key} in {dict}.keys()`")
    }

    fn autofix_title(&self) -> String {
        let InDictKeys { key, dict } = self;
        format!("Convert to `{key} in {dict}`")
    }
}

fn get_value_content_for_key_in_dict(
    locator: &Locator,
    stylist: &Stylist,
    expr: &Expr,
) -> Result<String> {
    let content = locator.slice(expr.range());
    let mut expression = match_expression(content)?;
    let call = match_call_mut(&mut expression)?;
    let attribute = match_attribute(&mut call.func)?;

    let mut state = CodegenState {
        default_newline: &stylist.line_ending(),
        default_indent: stylist.indentation(),
        ..CodegenState::default()
    };
    attribute.value.codegen(&mut state);

    Ok(state.to_string())
}

/// SIM118
fn key_in_dict(checker: &mut Checker, left: &Expr, right: &Expr, range: TextRange) {
    let Expr::Call(ast::ExprCall {
        func,
        args,
        keywords,
        range: _
    }) = &right else {
        return;
    };
    if !(args.is_empty() && keywords.is_empty()) {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func.as_ref() else {
        return;
    };
    if attr != "keys" {
        return;
    }

    // Slice exact content to preserve formatting.
    let left_content = checker.locator.slice(left.range());
    let value_content =
        match get_value_content_for_key_in_dict(checker.locator, checker.stylist, right) {
            Ok(value_content) => value_content,
            Err(err) => {
                error!("Failed to get value content for key in dict: {}", err);
                return;
            }
        };

    let mut diagnostic = Diagnostic::new(
        InDictKeys {
            key: left_content.to_string(),
            dict: value_content.clone(),
        },
        range,
    );
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
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
        TextRange::new(target.start(), iter.end()),
    );
}

/// SIM118 in a comparison
pub(crate) fn key_in_dict_compare(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    if !matches!(ops[..], [Cmpop::In]) {
        return;
    }

    if comparators.len() != 1 {
        return;
    }
    let right = comparators.first().unwrap();

    key_in_dict(checker, left, right, expr.range());
}
