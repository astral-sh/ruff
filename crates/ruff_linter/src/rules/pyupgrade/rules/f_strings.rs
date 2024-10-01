use std::borrow::Cow;

use anyhow::{Context, Result};
use rustc_hash::{FxHashMap, FxHashSet};

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_python_literal::format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};
use ruff_python_parser::TokenKind;
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

use crate::rules::pyflakes::format::FormatSummary;
use crate::rules::pyupgrade::helpers::{curly_escape, curly_unescape};

/// ## What it does
/// Checks for `str.format` calls that can be replaced with f-strings.
///
/// ## Why is this bad?
/// f-strings are more readable and generally preferred over `str.format`
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

impl Violation for FString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use f-string instead of `format` call")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Convert to f-string".to_string())
    }
}

/// Like [`FormatSummary`], but maps positional and keyword arguments to their
/// values. For example, given `{a} {b}".format(a=1, b=2)`, [`FormatSummary`]
/// would include `"a"` and `'b'` in `kwargs`, mapped to `1` and `2`
/// respectively.
#[derive(Debug)]
struct FormatSummaryValues<'a> {
    args: Vec<&'a Expr>,
    kwargs: FxHashMap<&'a str, &'a Expr>,
    auto_index: usize,
}

impl<'a> FormatSummaryValues<'a> {
    fn try_from_call(call: &'a ast::ExprCall, locator: &'a Locator) -> Option<Self> {
        let mut extracted_args: Vec<&Expr> = Vec::new();
        let mut extracted_kwargs: FxHashMap<&str, &Expr> = FxHashMap::default();

        for arg in &*call.arguments.args {
            if matches!(arg, Expr::Starred(..))
                || contains_quotes(locator.slice(arg))
                || locator.contains_line_break(arg.range())
            {
                return None;
            }
            extracted_args.push(arg);
        }
        for keyword in &*call.arguments.keywords {
            let Keyword {
                arg,
                value,
                range: _,
            } = keyword;
            let key = arg.as_ref()?;
            if contains_quotes(locator.slice(value)) || locator.contains_line_break(value.range()) {
                return None;
            }
            extracted_kwargs.insert(key, value);
        }

        if extracted_args.is_empty() && extracted_kwargs.is_empty() {
            return None;
        }

        Some(Self {
            args: extracted_args,
            kwargs: extracted_kwargs,
            auto_index: 0,
        })
    }

    /// Return the next positional index.
    fn arg_auto(&mut self) -> usize {
        let idx = self.auto_index;
        self.auto_index += 1;
        idx
    }

    /// Return the positional argument at the given index.
    fn arg_positional(&self, index: usize) -> Option<&Expr> {
        self.args.get(index).copied()
    }

    /// Return the keyword argument with the given name.
    fn arg_keyword(&self, key: &str) -> Option<&Expr> {
        self.kwargs.get(key).copied()
    }
}

/// Return `true` if the string contains quotes.
fn contains_quotes(string: &str) -> bool {
    string.contains(['\'', '"'])
}

enum FormatContext {
    /// The expression is used as a bare format spec (e.g., `{x}`).
    Bare,
    /// The expression is used with conversion flags, or attribute or subscript access
    /// (e.g., `{x!r}`, `{x.y}`, `{x[y]}`).
    Accessed,
}

/// Returns `true` if the expression should be parenthesized when used in an f-string.
fn parenthesize(expr: &Expr, text: &str, context: FormatContext) -> bool {
    match (context, expr) {
        // E.g., `x + y` should be parenthesized in `f"{(x + y)[0]}"`.
        (
            FormatContext::Accessed,
            Expr::BinOp(_)
            | Expr::UnaryOp(_)
            | Expr::BoolOp(_)
            | Expr::Named(_)
            | Expr::Compare(_)
            | Expr::If(_)
            | Expr::Lambda(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::YieldFrom(_)
            | Expr::Starred(_),
        ) => true,
        // E.g., `12` should be parenthesized in `f"{(12).real}"`.
        (
            FormatContext::Accessed,
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(..),
                ..
            }),
        ) => text.chars().all(|c| c.is_ascii_digit()),
        // E.g., `{x, y}` should be parenthesized in `f"{(x, y)}"`.
        (
            _,
            Expr::Generator(_)
            | Expr::Dict(_)
            | Expr::Set(_)
            | Expr::SetComp(_)
            | Expr::DictComp(_),
        ) => true,
        (_, Expr::Subscript(ast::ExprSubscript { value, .. })) => {
            matches!(
                value.as_ref(),
                Expr::Generator(_)
                    | Expr::Dict(_)
                    | Expr::Set(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            )
        }
        (_, Expr::Attribute(ast::ExprAttribute { value, .. })) => {
            matches!(
                value.as_ref(),
                Expr::Generator(_)
                    | Expr::Dict(_)
                    | Expr::Set(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            )
        }
        (_, Expr::Call(ast::ExprCall { func, .. })) => {
            matches!(
                func.as_ref(),
                Expr::Generator(_)
                    | Expr::Dict(_)
                    | Expr::Set(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            )
        }
        _ => false,
    }
}

/// Given an [`Expr`], format it for use in a formatted expression within an f-string.
fn formatted_expr<'a>(expr: &Expr, context: FormatContext, locator: &Locator<'a>) -> Cow<'a, str> {
    let text = locator.slice(expr);
    if parenthesize(expr, text, context) && !(text.starts_with('(') && text.ends_with(')')) {
        Cow::Owned(format!("({text})"))
    } else {
        Cow::Borrowed(text)
    }
}

#[derive(Debug, Clone)]
enum FStringConversion {
    /// The format string only contains literal parts and is empty.
    EmptyLiteral,
    /// The format string only contains literal parts and is non-empty.
    NonEmptyLiteral,
    /// The format call uses arguments with side effects which are repeated within the
    /// format string. For example: `"{x} {x}".format(x=foo())`.
    SideEffects,
    /// The format string should be converted to an f-string.
    Convert(String),
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum IndexOrKeyword {
    /// The field uses a positional index.
    Index(usize),
    /// The field uses a keyword name.
    Keyword(String),
}

impl FStringConversion {
    /// Convert a string `.format` call to an f-string.
    fn try_convert(
        range: TextRange,
        summary: &mut FormatSummaryValues,
        locator: &Locator,
    ) -> Result<Self> {
        let contents = locator.slice(range);

        // Strip the unicode prefix. It's redundant in Python 3, and invalid when used
        // with f-strings.
        let contents = if contents.starts_with('U') || contents.starts_with('u') {
            &contents[1..]
        } else {
            contents
        };

        // Temporarily strip the raw prefix, if present. It will be prepended to the result, before the
        // 'f', to match the prefix order both the Ruff formatter (and Black) use when formatting code.
        let raw = contents.starts_with('R') || contents.starts_with('r');
        let contents = if raw { &contents[1..] } else { contents };

        // Remove the leading and trailing quotes.
        let leading_quote = leading_quote(contents).context("Unable to identify leading quote")?;
        let trailing_quote =
            trailing_quote(contents).context("Unable to identify trailing quote")?;
        let contents = &contents[leading_quote.len()..contents.len() - trailing_quote.len()];

        // If the format string is empty, it doesn't need to be converted.
        if contents.is_empty() {
            return Ok(Self::EmptyLiteral);
        }

        // Parse the format string.
        let format_string = FormatString::from_str(contents)?;

        // If the format string contains only literal parts, it doesn't need to be converted.
        if format_string
            .format_parts
            .iter()
            .all(|part| matches!(part, FormatPart::Literal(..)))
        {
            return Ok(Self::NonEmptyLiteral);
        }

        let mut converted = String::with_capacity(contents.len());
        let mut seen = FxHashSet::default();
        for part in format_string.format_parts {
            match part {
                FormatPart::Field {
                    field_name,
                    conversion_spec,
                    format_spec,
                } => {
                    converted.push('{');

                    let field = FieldName::parse(&field_name)?;

                    // Map from field type to specifier.
                    let specifier = match field.field_type {
                        FieldType::Auto => IndexOrKeyword::Index(summary.arg_auto()),
                        FieldType::Index(index) => IndexOrKeyword::Index(index),
                        FieldType::Keyword(name) => IndexOrKeyword::Keyword(name),
                    };

                    let arg = match &specifier {
                        IndexOrKeyword::Index(index) => {
                            summary.arg_positional(*index).ok_or_else(|| {
                                anyhow::anyhow!("Positional argument {index} is missing")
                            })?
                        }
                        IndexOrKeyword::Keyword(name) => {
                            summary.arg_keyword(name).ok_or_else(|| {
                                anyhow::anyhow!("Keyword argument '{name}' is missing")
                            })?
                        }
                    };

                    // If the argument contains a side effect, and it's repeated in the format
                    // string, we can't convert the format string to an f-string. For example,
                    // converting `"{x} {x}".format(x=foo())` would result in `f"{foo()} {foo()}"`,
                    // which would call `foo()` twice.
                    if !seen.insert(specifier) {
                        if any_over_expr(arg, &Expr::is_call_expr) {
                            return Ok(Self::SideEffects);
                        }
                    }

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
                                let quote = match trailing_quote {
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
        let mut contents = String::with_capacity(usize::from(raw) + 1 + converted.len());
        if raw {
            contents.push('r');
        }
        contents.push('f');
        contents.push_str(leading_quote);
        contents.push_str(&converted);
        contents.push_str(trailing_quote);
        Ok(Self::Convert(contents))
    }
}

/// UP032
pub(crate) fn f_strings(checker: &mut Checker, call: &ast::ExprCall, summary: &FormatSummary) {
    if summary.has_nested_parts {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { value, .. }) = call.func.as_ref() else {
        return;
    };

    let Expr::StringLiteral(literal) = &**value else {
        return;
    };

    let Some(mut summary) = FormatSummaryValues::try_from_call(call, checker.locator()) else {
        return;
    };

    let mut patches: Vec<(TextRange, FStringConversion)> = vec![];
    let mut tokens = checker.tokens().in_range(call.func.range()).iter();
    let end = loop {
        let Some(token) = tokens.next() else {
            unreachable!("Should break from the `Tok::Dot` arm");
        };
        match token.kind() {
            TokenKind::Dot => {
                // ```
                // (
                //     "a"
                //     " {} "
                //     "b"
                // ).format(x)
                // ```
                // ^ Get the position of the character before the dot.
                //
                // We know that the expression is a string literal, so we can safely assume that the
                // dot is the start of an attribute access.
                break token.start();
            }
            TokenKind::String => {
                match FStringConversion::try_convert(token.range(), &mut summary, checker.locator())
                {
                    // If the format string contains side effects that would need to be repeated,
                    // we can't convert it to an f-string.
                    Ok(FStringConversion::SideEffects) => return,
                    // If any of the segments fail to convert, then we can't convert the entire
                    // expression.
                    Err(_) => return,
                    // Otherwise, push the conversion to be processed later.
                    Ok(conversion) => patches.push((token.range(), conversion)),
                }
            }
            _ => {}
        }
    };
    if patches.is_empty() {
        return;
    }

    let mut contents = String::with_capacity(checker.locator().slice(call).len());
    let mut prev_end = call.start();
    for (range, conversion) in patches {
        let fstring = match conversion {
            FStringConversion::Convert(fstring) => Some(fstring),
            FStringConversion::EmptyLiteral => None,
            FStringConversion::NonEmptyLiteral => {
                // Convert escaped curly brackets e.g. `{{` to `{` in literal string parts
                Some(curly_unescape(checker.locator().slice(range)).to_string())
            }
            // We handled this in the previous loop.
            FStringConversion::SideEffects => unreachable!(),
        };
        if let Some(fstring) = fstring {
            contents.push_str(
                checker
                    .locator()
                    .slice(TextRange::new(prev_end, range.start())),
            );
            contents.push_str(&fstring);
        }
        prev_end = range.end();
    }
    contents.push_str(checker.locator().slice(TextRange::new(prev_end, end)));

    // If necessary, add a space between any leading keyword (`return`, `yield`, `assert`, etc.)
    // and the string. For example, `return"foo"` is valid, but `returnf"foo"` is not.
    let existing = checker.locator().slice(TextRange::up_to(call.start()));
    if existing
        .chars()
        .last()
        .is_some_and(|char| char.is_ascii_alphabetic())
    {
        contents.insert(0, ' ');
    }

    // Finally, avoid refactors that would introduce a runtime error.
    // For example, Django's `gettext` supports `format`-style arguments, but not f-strings.
    // See: https://docs.djangoproject.com/en/4.2/topics/i18n/translation
    if checker.semantic().current_expressions().any(|expr| {
        expr.as_call_expr().is_some_and(|call| {
            checker
                .semantic()
                .resolve_qualified_name(call.func.as_ref())
                .map_or(false, |qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["django", "utils", "translation", "gettext" | "gettext_lazy"]
                    )
                })
        })
    }) {
        return;
    }

    let mut diagnostic = Diagnostic::new(FString, call.range());

    // Avoid fix if there are comments within the call:
    // ```
    // "{}".format(
    //     0,  # 0
    // )
    // ```
    let has_comments = checker.comment_ranges().intersects(call.arguments.range());

    if !has_comments {
        if contents.is_empty() {
            // Ex) `''.format(self.project)`
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                checker.locator().slice(literal).to_string(),
                call.range(),
            )));
        } else {
            diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                contents,
                call.range(),
            )));
        }
    };
    checker.diagnostics.push(diagnostic);
}
