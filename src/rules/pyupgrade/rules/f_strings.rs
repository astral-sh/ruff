use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use rustc_hash::FxHashMap;
use rustpython_ast::{Constant, Expr, ExprKind, KeywordData};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, RuleCode};
use crate::rules::pyflakes::format::FormatSummary;
use crate::violations;

static NAME_SPECIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{(?P<name>[^\W0-9]\w*)?(?P<fmt>.*?)}").unwrap());

static HAS_BRACKETS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[.*]").unwrap());

/// Like [`FormatSummary`], but maps positional and keyword arguments to their
/// values. For example, given `{a} {b}".format(a=1, b=2)`, `FormatFunction`
/// would include `"a"` and `'b'` in `kwargs`, mapped to `1` and `2`
/// respectively.
#[derive(Debug)]
struct FormatSummaryValues<'a> {
    args: Vec<String>,
    kwargs: FxHashMap<&'a str, String>,
}

impl<'a> FormatSummaryValues<'a> {
    fn try_from_expr(checker: &'a Checker, expr: &'a Expr) -> Option<Self> {
        let mut extracted_args: Vec<String> = Vec::new();
        let mut extracted_kwargs: FxHashMap<&str, String> = FxHashMap::default();
        if let ExprKind::Call { args, keywords, .. } = &expr.node {
            for arg in args {
                let arg = checker
                    .locator
                    .slice_source_code_range(&Range::from_located(arg));
                if contains_invalids(&arg) {
                    return None;
                }
                extracted_args.push(arg.to_string());
            }
            for keyword in keywords {
                let KeywordData { arg, value } = &keyword.node;
                if let Some(key) = arg {
                    let kwarg = checker
                        .locator
                        .slice_source_code_range(&Range::from_located(value));
                    if contains_invalids(&kwarg) {
                        return None;
                    }
                    extracted_kwargs.insert(key, kwarg.to_string());
                }
            }
        }

        if extracted_args.is_empty() && extracted_kwargs.is_empty() {
            return None;
        }

        Some(Self {
            args: extracted_args,
            kwargs: extracted_kwargs,
        })
    }

    fn consume_arg(&mut self) -> Option<String> {
        if self.args.is_empty() {
            None
        } else {
            Some(self.args.remove(0))
        }
    }

    fn consume_kwarg(&mut self, key: &str) -> Option<String> {
        self.kwargs.remove(key)
    }

    /// Return `true` if the statement and function call match.
    fn validate(&self, summary: &FormatSummary) -> bool {
        let mut self_keys = self.kwargs.clone().into_keys().collect::<Vec<_>>();
        self_keys.sort_unstable();

        let mut summary_keys = summary.keywords.clone();
        summary_keys.sort();

        summary.autos.len() == self.args.len() && self_keys == summary_keys
    }
}

/// Return `true` if the string contains characters that are forbidden in
/// argument identifier.
fn contains_invalids(string: &str) -> bool {
    string.contains('*')
        || string.contains('\'')
        || string.contains('"')
        || string.contains("await")
}

/// Extract the format spec from a regex [`Captures`] object.
fn extract_format_spec(caps: &Captures, target: &str) -> Result<String> {
    let Some(match_) = caps.name(target) else {
        return Err(anyhow!("No match for target: {}", target));
    };
    let match_ = match_.as_str();
    if HAS_BRACKETS.is_match(match_) {
        return Err(anyhow!("Invalid match for target: {}", target));
    }
    Ok(match_.to_string())
}

// See: https://github.com/rust-lang/regex/issues/648
fn replace_all(
    re: &Regex,
    haystack: &str,
    mut replacement: impl FnMut(&Captures) -> Result<String>,
) -> Result<String> {
    let mut new = String::with_capacity(haystack.len());
    let mut last_match = 0;
    for caps in re.captures_iter(haystack) {
        let m = caps.get(0).unwrap();
        new.push_str(&haystack[last_match..m.start()]);
        new.push_str(&replacement(&caps)?);
        last_match = m.end();
    }
    new.push_str(&haystack[last_match..]);
    Ok(new)
}

/// Generate an f-string from an [`Expr`].
fn try_convert_to_f_string(checker: &Checker, expr: &Expr) -> Option<String> {
    let ExprKind::Call { func, .. } = &expr.node else {
        return None;
    };
    let ExprKind::Attribute { value, .. } = &func.node else {
        return None;
    };
    let ExprKind::Constant { value, .. } = &value.node else {
        return None;
    };
    let Constant::Str(string) = value else {
        return None;
    };

    let contents = string.to_string();
    if contents.is_empty() {
        return None;
    }

    let Some(mut summary) = FormatSummaryValues::try_from_expr(checker, expr) else {
        return None;
    };

    // You can't return a function from inside a closure, so we just record that
    // there was an error.
    let clean_string = replace_all(&NAME_SPECIFIER, &contents, |caps: &Captures| {
        if let Some(name) = caps.name("name") {
            let Some(value) = summary.consume_kwarg(name.as_str()) else {
                return Err(anyhow!("Missing kwarg"));
            };
            let Ok(format_spec) = extract_format_spec(caps, "fmt") else {
                return Err(anyhow!("Missing caps"));
            };
            Ok(format!("{{{value}{format_spec}}}"))
        } else {
            let Some(value) = summary.consume_arg() else {
                return Err(anyhow!("Missing arg"));
            };
            let Ok(format_spec) = extract_format_spec(caps, "fmt") else {
                return Err(anyhow!("Missing caps"));
            };
            Ok(format!("{{{value}{format_spec}}}"))
        }
    })
    .ok()?;
    Some(format!("f\"{clean_string}\""))
}

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
    if summary.has_nested_parts {
        return;
    }
    if !summary.indexes.is_empty() {
        return;
    }

    let existing = checker
        .locator
        .slice_source_code_range(&Range::from_located(expr));

    // Avoid refactoring multi-line strings.
    if existing.contains('\n') {
        return;
    }

    // Currently, the only issue we know of is in LibCST:
    // https://github.com/Instagram/LibCST/issues/846
    let Some(contents) = try_convert_to_f_string(checker, expr) else {
        return;
    };

    // Avoid refactors that increase the resulting string length.
    if contents.len() > existing.len() {
        return;
    }

    let mut diagnostic = Diagnostic::new(violations::FString, Range::from_located(expr));
    if checker.patch(&RuleCode::UP032) {
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
    fn test_validate() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string(), "d".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatSummaryValues {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("c", "e".to_string()), ("d", "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
        let checks_out = form_func.validate(&summary);
        assert!(checks_out);
    }

    #[test]
    fn test_validate_unequal_args() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatSummaryValues {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("c", "e".to_string()), ("d", "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
        let checks_out = form_func.validate(&summary);
        assert!(!checks_out);
    }

    #[test]
    fn test_validate_different_kwargs() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string(), "d".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatSummaryValues {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("c", "e".to_string()), ("e", "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
        let checks_out = form_func.validate(&summary);
        assert!(!checks_out);
    }

    #[test]
    fn test_validate_kwargs_same_diff_order() {
        let summary = FormatSummary {
            autos: vec![0, 1],
            keywords: vec!["c".to_string(), "d".to_string()],
            has_nested_parts: false,
            indexes: vec![],
        };
        let form_func = FormatSummaryValues {
            args: vec!["a".to_string(), "b".to_string()],
            kwargs: [("d", "e".to_string()), ("c", "f".to_string())]
                .iter()
                .cloned()
                .collect(),
        };
        let checks_out = form_func.validate(&summary);
        assert!(checks_out);
    }
}
