use once_cell::sync::Lazy;
use regex::{Regex, Captures};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pyflakes::format::FormatSummary;
use crate::violations;
use rustpython_ast::{Expr, ExprKind, KeywordData, Constant};
use std::collections::HashMap;

// Checks for curly brackets. Inside these brackets this checks for an optional valid python name
// and any format specifiers afterwards
static NAME_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<name>[^\W0-9]\w*)?(?P<fmt>.*?)}").unwrap());

#[derive(Debug)]
struct FormatFunction {
    args: Vec<String>,
    kwargs: HashMap<String, String>,
}

impl FormatFunction {
    fn from_expr(checker: &mut Checker, expr: &Expr) -> Self {
        let mut final_args: Vec<String> = Vec::new();
        let mut final_kwargs: HashMap<String, String> = HashMap::new();
        if let ExprKind::Call { args, keywords, .. } = &expr.node {
            for arg in args {
                let arg_range = Range::from_located(&arg);
                let arg_string = checker.locator.slice_source_code_range(&arg_range);
                final_args.push(arg_string.to_string());
            }

            for keyword in keywords {
                let KeywordData { arg, value } = &keyword.node;
                if let Some(key) = arg {
                    let kwarg_range = Range::from_located(&value);
                    let kwarg_string = checker.locator.slice_source_code_range(&kwarg_range);
                    final_kwargs.insert(key.to_string(), kwarg_string.to_string());
                }
            }
        }
        Self {
            args: final_args,
            kwargs: final_kwargs,
        }
    }

    /// Returns true if args and kwargs are empty
    fn is_empty(&self) -> bool {
        self.args.is_empty() && self.kwargs.is_empty()
    }

    fn consume_arg(&mut self) -> Option<String> {
        if self.args.len() > 0 {
            Some(self.args.remove(0))
        } else {
            None
        }
    }

    fn get_kwarg(&self, key: &str) -> Option<String> {
        self.kwargs.get(key).cloned()
    }

    /// Returns true if the statement and function call match, and false if not
    fn check_with_summary(&self, summary: &FormatSummary) -> bool {
        let mut self_keys = self.kwargs.clone().into_keys().collect::<Vec<_>>();
        self_keys.sort();
        let mut summary_keys = summary.keywords.clone();
        summary_keys.sort();
        summary.autos.len() == self.args.len() && self_keys == summary_keys
    }
}

fn create_new_string(expr: &Expr, function: &mut FormatFunction) -> Option<String> {
    let mut new_string = String::new();
    if let ExprKind::Call { func, .. } = &expr.node {
        if let ExprKind::Attribute { value, .. }  = &func.node {
            if let ExprKind::Constant { value, .. } = & value.node {
                if let Constant::Str(string) = value {
                    new_string = string.to_string();
                }
            }
        }
    }
    // If we didn't get a valid string, return an empty string
    if new_string.is_empty() {
        return None;
    }
    // You can't return a function from inside a closure, so we just record that there was an error
    let mut had_error = false;
    let clean_string = NAME_SPECIFIER.replace_all(&new_string, |caps: &Captures| {
        if let Some(name) = caps.name("name") {
            let kwarg = match function.get_kwarg(name.as_str()) {
                None => {
                    had_error = true;
                    return String::new();
                }
                Some(item) => item
            };
            let second_part = match caps.name("fmt") {
                None => {
                    had_error = true;
                    return String::new();
                }
                Some(item) => item.as_str()
            };
            format!("{{{}{}}}", kwarg, second_part)
        } else {
            let arg = match function.consume_arg() {
                None => {
                    had_error = true;
                    return String::new();
                }
                Some(item) => item
            };
            let second_part = match caps.name("fmt") {
                None => {
                    had_error = true;
                    return String::new();
                }
                Some(item) => item.as_str()
            };
            format!("{{{}{}}}", arg, second_part)
        }
    });
    if had_error {
        return None
    }
    Some(format!("f\"{}\"", clean_string))
}

fn generate_f_string(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) -> Option<String> {
    let mut original_call = FormatFunction::from_expr(checker, expr);
    // We do not need to make changes if there are no arguments (let me know if you want me to
    // change this to removing the .format() and doing nothing else, but that seems like it could
    // cause issues)
    if original_call.is_empty() {
        return None;
    }
    // If the actual string and the format function do not match, we cannot operate
    if !original_call.check_with_summary(summary) {
        return None;
    }
    create_new_string(expr, &mut original_call)
}

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
    if summary.has_nested_parts {
        return;
    }
    // UP030 already removes the indexes, so we should not need to worry about the complexity
    if !summary.indexes.is_empty() {
        return;
    }
    // Currently, the only issue we know of is in LibCST:
    // https://github.com/Instagram/LibCST/issues/846
    let contents = match generate_f_string(checker, summary, expr) {
        None => return,
        Some(items) => items,
    };
    let mut diagnostic = Diagnostic::new(violations::FString, Range::from_located(expr));
    if checker.patch(diagnostic.kind.code()) {
        diagnostic.amend(Fix::replacement(
            contents,
            expr.location,
            expr.end_location.unwrap(),
        ));
    };
    checker.diagnostics.push(diagnostic);
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_with_summary() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string(), "d".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatFunction {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("c".to_string(), "e".to_string()), ("d".to_string(), "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
    let checks_out = form_func.check_with_summary(&summary);
    assert!(checks_out);
    }

    #[test]
    fn test_check_with_summary_unuequal_args() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatFunction {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("c".to_string(), "e".to_string()), ("d".to_string(), "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
    let checks_out = form_func.check_with_summary(&summary);
    assert!(!checks_out);
    }

    #[test]
    fn test_check_with_summary_different_kwargs() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string(), "d".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatFunction {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("c".to_string(), "e".to_string()), ("e".to_string(), "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
    let checks_out = form_func.check_with_summary(&summary);
    assert!(!checks_out);
    }

    #[test]
    fn test_check_with_summary_kwargs_same_diff_order() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string(), "d".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatFunction {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("d".to_string(), "e".to_string()), ("c".to_string(), "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
    let checks_out = form_func.check_with_summary(&summary);
    assert!(checks_out);
    }
}
