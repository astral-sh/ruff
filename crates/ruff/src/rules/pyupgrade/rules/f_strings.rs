use std::borrow::Cow;

use ruff_text_size::TextRange;
use rustc_hash::FxHashMap;
use rustpython_format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};
use rustpython_parser::ast::{self, Constant, Expr, Keyword, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::source_code::Locator;
use ruff_python_ast::str::{is_implicit_concatenation, leading_quote, trailing_quote};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::pyflakes::format::FormatSummary;
use crate::rules::pyupgrade::helpers::curly_escape;

/// ## What it does
/// Checks for `str#format` calls that can be replaced with f-strings.
///
/// ## Why is this bad?
/// f-strings are more readable and generally preferred over `str#format`
/// calls.
///
/// ## Example
/// ```python
/// "{}".format(foo)
/// ```
///
/// Use instead:
/// ```python
/// f"{foo}"
/// ```
///
/// ## References
/// - [Python documentation: f-strings](https://docs.python.org/3/reference/lexical_analysis.html#f-strings)
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
    args: Vec<&'a Expr>,
    kwargs: FxHashMap<&'a str, &'a Expr>,
}

impl<'a> FormatSummaryValues<'a> {
    fn try_from_expr(expr: &'a Expr, locator: &'a Locator) -> Option<Self> {
        let mut extracted_args: Vec<&Expr> = Vec::new();
        let mut extracted_kwargs: FxHashMap<&str, &Expr> = FxHashMap::default();
        if let Expr::Call(ast::ExprCall { args, keywords, .. }) = expr {
            for arg in args {
                if contains_invalids(locator.slice(arg.range())) {
                    return None;
                }
                extracted_args.push(arg);
            }
            for keyword in keywords {
                let Keyword {
                    arg,
                    value,
                    range: _,
                } = keyword;
                if let Some(key) = arg {
                    if contains_invalids(locator.slice(value.range())) {
                        return None;
                    }
                    extracted_kwargs.insert(key, value);
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

    fn consume_next(&mut self) -> Option<&Expr> {
        if self.args.is_empty() {
            None
        } else {
            Some(self.args.remove(0))
        }
    }

    fn consume_arg(&mut self, index: usize) -> Option<&Expr> {
        if self.args.len() > index {
            Some(self.args.remove(index))
        } else {
            None
        }
    }

    fn consume_kwarg(&mut self, key: &str) -> Option<&Expr> {
        self.kwargs.remove(key)
    }
}

/// Return `true` if the string contains characters that are forbidden by
/// argument identifiers.
fn contains_invalids(string: &str) -> bool {
    string.contains('*')
        || string.contains('\'')
        || string.contains('"')
        || string.contains("await")
}

enum FormatContext {
    /// The expression is used as a bare format spec (e.g., `{x}`).
    Bare,
    /// The expression is used with conversion flags, or attribute or subscript access
    /// (e.g., `{x!r}`, `{x.y}`, `{x[y]}`).
    Accessed,
}

/// Given an [`Expr`], format it for use in a formatted expression within an f-string.
fn formatted_expr<'a>(expr: &Expr, context: FormatContext, locator: &Locator<'a>) -> Cow<'a, str> {
    let text = locator.slice(expr.range());
    let parenthesize = match (context, expr) {
        // E.g., `x + y` should be parenthesized in `f"{(x + y)[0]}"`.
        (
            FormatContext::Accessed,
            Expr::BinOp(_)
            | Expr::UnaryOp(_)
            | Expr::BoolOp(_)
            | Expr::NamedExpr(_)
            | Expr::Compare(_)
            | Expr::IfExp(_)
            | Expr::Lambda(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::Starred(_),
        ) => true,
        // E.g., `12` should be parenthesized in `f"{(12).real}"`.
        (
            FormatContext::Accessed,
            Expr::Constant(ast::ExprConstant {
                value: Constant::Int(..),
                ..
            }),
        ) => text.chars().all(|c| c.is_ascii_digit()),
        // E.g., `{x, y}` should be parenthesized in `f"{(x, y)}"`.
        (
            _,
            Expr::GeneratorExp(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_),
        ) => true,
        _ => false,
    };
    if parenthesize && !text.starts_with('(') && !text.ends_with(')') {
        Cow::Owned(format!("({text})"))
    } else {
        Cow::Borrowed(text)
    }
}

/// Generate an f-string from an [`Expr`].
fn try_convert_to_f_string(expr: &Expr, locator: &Locator) -> Option<String> {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return None;
    };
    let Expr::Attribute(ast::ExprAttribute { value, .. }) = func.as_ref() else {
        return None;
    };
    if !matches!(
        value.as_ref(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(..),
            ..
        }),
    ) {
        return None;
    };

    let Some(mut summary) = FormatSummaryValues::try_from_expr(expr, locator) else {
        return None;
    };

    let contents = locator.slice(value.range());

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
                let arg = match field.field_type {
                    FieldType::Auto => summary.consume_next(),
                    FieldType::Index(index) => summary.consume_arg(index),
                    FieldType::Keyword(name) => summary.consume_kwarg(&name),
                }?;
                converted.push_str(&formatted_expr(
                    arg,
                    if field.parts.is_empty() {
                        FormatContext::Bare
                    } else {
                        FormatContext::Accessed
                    },
                    locator,
                ));

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
    let Some(mut contents) = try_convert_to_f_string(expr, checker.locator) else {
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
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            contents,
            expr.range(),
        )));
    };
    checker.diagnostics.push(diagnostic);
}
