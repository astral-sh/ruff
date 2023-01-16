use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pyflakes::format::FormatSummary;
use crate::violations;
use rustpython_ast::{Expr, ExprKind, KeywordData};
use std::collections::HashMap;

#[derive(Debug)]
struct FormatFunction {
    args: Vec<String>,
    kwargs: HashMap<String, String>,
}

impl FormatFunction {
    fn from_expr(expr: &Expr) -> Self {
        let mut final_args: Vec<String> = Vec::new();
        let mut final_kwargs: HashMap<String, String> = HashMap::new();
        if let ExprKind::Call { args, keywords, .. } = &expr.node {
            for arg in args {
                if let ExprKind::Name { id, .. } = &arg.node {
                    final_args.push(id.to_string())
                }
            }

            for keyword in keywords {
                let KeywordData { arg, value } = &keyword.node;
                if let ExprKind::Name { id, .. } = &value.node {
                    if let Some(key) = arg {
                        final_kwargs.insert(key.to_string(), id.to_string());
                    }
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

    fn add_arg(&mut self, arg: String) {
        self.args.push(arg);
    }

    fn add_kwarg(&mut self, key: String, value: String) {
        self.kwargs.insert(key, value);
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

fn generate_f_string(summary: &FormatSummary, expr: &Expr) -> Option<String> {
    let mut original_call = FormatFunction::from_expr(expr);
    println!("{:?}", original_call);
    Some(String::new())
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
    let contents = match generate_f_string(summary, expr) {
        None => return,
        Some(items) => items,
    };
    println!("WE HERE");
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
