use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashMap;
use rustpython_common::format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};
use rustpython_parser::ast::{Constant, Expr, ExprKind, KeywordData};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::rules::pydocstyle::helpers::{leading_quote, trailing_quote};
use crate::rules::pyflakes::format::FormatSummary;
use crate::rules::pyupgrade::helpers::curly_escape;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct FString;
);
impl AlwaysAutofixableViolation for FString {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use f-string instead of `format` call")
    }

    fn autofix_title(&self) -> String {
        "Convert to f-string".to_string()
    }
}

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
                if contains_invalids(arg) {
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
                    if contains_invalids(kwarg) {
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

    fn consume_next(&mut self) -> Option<String> {
        if self.args.is_empty() {
            None
        } else {
            Some(self.args.remove(0))
        }
    }

    fn consume_arg(&mut self, index: usize) -> Option<String> {
        if self.args.len() > index {
            Some(self.args.remove(index))
        } else {
            None
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

    let Some(mut summary) = FormatSummaryValues::try_from_expr(checker, expr) else {
        return None;
    };

    let contents = checker
        .locator
        .slice_source_code_range(&Range::from_located(value));

    // Tokenize: we need to avoid trying to fix implicit string concatenations.
    if lexer::make_tokenizer(contents)
        .flatten()
        .filter(|(_, tok, _)| matches!(tok, Tok::String { .. }))
        .count()
        > 1
    {
        return None;
    }

    // Strip the unicode prefix. It's redundant in Python 3, and invalid when used
    // with f-strings.
    let contents = if contents.starts_with('U') || contents.starts_with('u') {
        &contents[1..]
    } else {
        contents
    };
    if contents.is_empty() {
        return None;
    }

    // Remove the leading and trailing quotes.
    let Some(leading_quote) = leading_quote(contents) else {
        return None;
    };
    let Some(trailing_quote) = trailing_quote(contents) else {
        return None;
    };
    let contents = &contents[leading_quote.len()..contents.len() - trailing_quote.len()];

    // Parse the format string.
    let Ok(format_string) = FormatString::from_str(contents) else {
        return None;
    };

    let mut converted = String::with_capacity(contents.len());
    for part in format_string.format_parts {
        match part {
            FormatPart::Field {
                field_name,
                preconversion_spec,
                format_spec,
            } => {
                converted.push('{');

                let field = FieldName::parse(&field_name).ok()?;
                match field.field_type {
                    FieldType::Auto => {
                        let Some(arg) = summary.consume_next() else {
                            return None;
                        };
                        converted.push_str(&arg);
                    }
                    FieldType::Index(index) => {
                        let Some(arg) = summary.consume_arg(index) else {
                            return None;
                        };
                        converted.push_str(&arg);
                    }
                    FieldType::Keyword(name) => {
                        let Some(arg) = summary.consume_kwarg(&name) else {
                            return None;
                        };
                        converted.push_str(&arg);
                    }
                }

                for part in field.parts {
                    match part {
                        FieldNamePart::Attribute(name) => {
                            converted.push('.');
                            converted.push_str(&name);
                        }
                        FieldNamePart::Index(index) => {
                            converted.push('[');
                            converted.push_str(index.to_string().as_str());
                            converted.push(']');
                        }
                        FieldNamePart::StringIndex(index) => {
                            converted.push('[');
                            converted.push_str(&index);
                            converted.push(']');
                        }
                    }
                }

                if let Some(preconversion_spec) = preconversion_spec {
                    converted.push('!');
                    converted.push(preconversion_spec);
                }

                if !format_spec.is_empty() {
                    converted.push(':');
                    converted.push_str(&format_spec);
                }

                converted.push('}');
            }
            FormatPart::Literal(value) => {
                converted.push_str(&curly_escape(&value));
            }
        }
    }

    // Construct the format string.
    let mut contents = String::with_capacity(1 + converted.len());
    contents.push('f');
    contents.push_str(leading_quote);
    contents.push_str(&converted);
    contents.push_str(trailing_quote);
    Some(contents)
}

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, summary: &FormatSummary, expr: &Expr) {
    if summary.has_nested_parts {
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

    let mut diagnostic = Diagnostic::new(FString, Range::from_located(expr));
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.amend(Fix::replacement(
            contents,
            expr.location,
            expr.end_location.unwrap(),
        ));
    };
    checker.diagnostics.push(diagnostic);
}
