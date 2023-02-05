use crate::define_violation;
use crate::violation::AlwaysAutofixableViolation;
use ruff_macros::derive_message_formats;
use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::leading_quote;

const STRIPS: &[&str] = &["strip", "lstrip", "rstrip"];

define_violation!(
    pub struct BadStrStripCall;
);
impl AlwaysAutofixableViolation for BadStrStripCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Strip string contains duplicate characters")
    }

    fn autofix_title(&self) -> String {
        "Removed duplicated characters".to_string()
    }
}

fn remove_duplicates(s: &str) -> String {
    let mut set = std::collections::HashSet::new();
    let mut result = String::new();
    for c in s.chars() {
        if set.insert(c) {
            result.push(c);
        }
    }
    result
}

/// PLE1310
pub fn bad_str_strip_call(checker: &mut Checker, func: &Expr, args: &[Expr]) {
    if let ExprKind::Attribute { value, attr, .. } = &func.node {
        if let ExprKind::Constant {
            value: Constant::Str(_) | Constant::Bytes(_),
            ..
        } = &value.node
        {
            if STRIPS.contains(&attr.as_str()) {
                if let Some(arg) = args.get(0) {
                    if let ExprKind::Constant {
                        value: Constant::Str(item),
                        ..
                    } = &arg.node
                    {
                        let cleaned = remove_duplicates(item);
                        let module_text = checker
                            .locator
                            .slice_source_code_range(&Range::from_located(arg));
                        let quote = match leading_quote(module_text) {
                            Some(item) => item,
                            None => return,
                        };
                        if &cleaned != item {
                            let mut diagnostic =
                                Diagnostic::new(BadStrStripCall, Range::from_located(arg));
                            if checker.patch(diagnostic.kind.rule()) {
                                diagnostic.amend(Fix::replacement(
                                    format!("{quote}{cleaned}{quote}"),
                                    arg.location,
                                    arg.end_location.unwrap(),
                                ));
                            };
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
            }
        }
    }
}
