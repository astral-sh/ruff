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
    if !matches!(
        &value.node,
        ExprKind::Constant {
            value: Constant::Str(..),
            ..
        },
    ) {
        return None;
    };

    let contents = checker
        .locator
        .slice_source_code_range(&Range::from_located(value));
    let contents = if contents.starts_with('U') || contents.starts_with('u') {
        &contents[1..]
    } else {
        &contents
    };
    if contents.is_empty() {
        return None;
    }

    let Some(mut summary) = FormatSummaryValues::try_from_expr(checker, expr) else {
        return None;
    };

    let converted = replace_all(&NAME_SPECIFIER, contents, |caps: &Captures| {
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

    // Construct the format string.
    let mut contents = String::with_capacity(1 + converted.len());
    contents.push('f');
    contents.push_str(&converted);
    Some(contents)
}

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
    if summary.has_nested_parts {
        return;
    }
    if !summary.indexes.is_empty() {
        return;
    }

    // Avoid refactoring multi-line strings.
    if expr.location.row() != expr.end_location.unwrap().row() {
        return;
    }

    // Currently, the only issue we know of is in LibCST:
    // https://github.com/Instagram/LibCST/issues/846
    let Some(contents) = try_convert_to_f_string(checker, expr) else {
        return;
    };

    // Avoid refactors that increase the resulting string length.
    let existing = checker
        .locator
        .slice_source_code_range(&Range::from_located(expr));
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
