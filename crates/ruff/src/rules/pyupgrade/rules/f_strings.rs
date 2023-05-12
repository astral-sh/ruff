use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;
use rustpython_common::format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};
use rustpython_parser::ast::{self, Constant, Expr, ExprKind, KeywordData};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{is_implicit_concatenation, leading_quote, trailing_quote};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pyflakes::format::FormatSummary;
use crate::rules::pyupgrade::helpers::curly_escape;

#[violation]
pub struct FString;

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
        if let ExprKind::Call(ast::ExprCall { args, keywords, .. }) = &expr.node {
            for arg in args {
                let arg = checker.locator.slice(arg.range());
                if contains_invalids(arg) {
                    return None;
                }
                extracted_args.push(arg.to_string());
            }
            for keyword in keywords {
                let KeywordData { arg, value } = &keyword.node;
                if let Some(key) = arg {
                    let kwarg = checker.locator.slice(value.range());
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
    let ExprKind::Call(ast::ExprCall { func, .. }) = &expr.node else {
        return None;
    };
    let ExprKind::Attribute(ast::ExprAttribute { value, .. }) = &func.node else {
        return None;
    };
    if !matches!(
        &value.node,
        ExprKind::Constant(ast::ExprConstant {
            value: Constant::Str(..),
            ..
        }),
    ) {
        return None;
    };

    let Some(mut summary) = FormatSummaryValues::try_from_expr(checker, expr) else {
        return None;
    };

    let contents = checker.locator.slice(value.range());

    // Skip implicit string concatenations.
    if is_implicit_concatenation(contents) {
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
                conversion_spec,
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
                            let quote = match *trailing_quote {
                                "'" | "'''" | "\"\"\"" => '"',
                                "\"" => '\'',
                                _ => unreachable!("invalid trailing quote"),
                            };
                            converted.push('[');
                            converted.push(quote);
                            converted.push_str(&index);
                            converted.push(quote);
                            converted.push(']');
                        }
                    }
                }

                if let Some(conversion_spec) = conversion_spec {
                    converted.push('!');
                    converted.push(conversion_spec);
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
    if checker.locator.contains_line_break(expr.range()) {
        return;
    }

    // Currently, the only issue we know of is in LibCST:
    // https://github.com/Instagram/LibCST/issues/846
    let Some(mut contents) = try_convert_to_f_string(checker, expr) else {
        return;
    };

    // Avoid refactors that increase the resulting string length.
    let existing = checker.locator.slice(expr.range());
    if contents.len() > existing.len() {
        return;
    }

    // If necessary, add a space between any leading keyword (`return`, `yield`, `assert`, etc.)
    // and the string. For example, `return"foo"` is valid, but `returnf"foo"` is not.
    let existing = checker.locator.slice(TextRange::up_to(expr.start()));
    if existing
        .chars()
        .last()
        .map_or(false, |char| char.is_ascii_alphabetic())
    {
        contents.insert(0, ' ');
    }

    let mut diagnostic = Diagnostic::new(FString, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    };
    checker.diagnostics.push(diagnostic);
}
