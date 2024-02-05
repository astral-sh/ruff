use std::borrow::Cow;

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;

use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::str::{leading_quote, trailing_quote};
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_python_literal::format::{
    FieldName, FieldNamePart, FieldType, FormatPart, FormatString, FromTemplate,
};
use ruff_python_parser::{lexer, Mode, Tok};
use ruff_source_file::Locator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::fits_or_shrinks;

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
/// values. For example, given `{a} {b}".format(a=1, b=2)`, `FormatFunction`
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

        for arg in &call.arguments.args {
            if matches!(arg, Expr::Starred(..))
                || contains_quotes(locator.slice(arg))
                || locator.contains_line_break(arg.range())
            {
                return None;
            }
            extracted_args.push(arg);
        }
        for keyword in &call.arguments.keywords {
            let Keyword {
                arg,
                value,
                range: _,
            } = keyword;
            let Some(key) = arg else {
                return None;
            };
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

    /// Return the next positional argument.
    fn arg_auto(&mut self) -> Option<&Expr> {
        let idx = self.auto_index;
        self.auto_index += 1;
        self.arg_positional(idx)
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
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(..),
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
        (_, Expr::Subscript(ast::ExprSubscript { value, .. })) => {
            matches!(
                value.as_ref(),
                Expr::GeneratorExp(_)
                    | Expr::Dict(_)
                    | Expr::Set(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            )
        }
        (_, Expr::Attribute(ast::ExprAttribute { value, .. })) => {
            matches!(
                value.as_ref(),
                Expr::GeneratorExp(_)
                    | Expr::Dict(_)
                    | Expr::Set(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            )
        }
        (_, Expr::Call(ast::ExprCall { func, .. })) => {
            matches!(
                func.as_ref(),
                Expr::GeneratorExp(_)
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

/// Convert a string `.format` call to an f-string.
///
/// Returns `None` if the string does not require conversion, and `Err` if the conversion
/// is not possible.
fn try_convert_to_f_string(
    range: TextRange,
    summary: &mut FormatSummaryValues,
    locator: &Locator,
) -> Result<Option<String>> {
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
    let trailing_quote = trailing_quote(contents).context("Unable to identify trailing quote")?;
    let contents = &contents[leading_quote.len()..contents.len() - trailing_quote.len()];
    if contents.is_empty() {
        return Ok(None);
    }

    // Parse the format string.
    let format_string = FormatString::from_str(contents)?;

    if format_string
        .format_parts
        .iter()
        .all(|part| matches!(part, FormatPart::Literal(..)))
    {
        return Ok(None);
    }

    let mut converted = String::with_capacity(contents.len());
    for part in format_string.format_parts {
        match part {
            FormatPart::Field {
                field_name,
                conversion_spec,
                format_spec,
            } => {
                converted.push('{');

                let field = FieldName::parse(&field_name)?;
                let arg = match field.field_type {
                    FieldType::Auto => summary.arg_auto(),
                    FieldType::Index(index) => summary.arg_positional(index),
                    FieldType::Keyword(name) => summary.arg_keyword(&name),
                }
                .context("Unable to parse field")?;
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
    Ok(Some(contents))
}

/// UP032
pub(crate) fn f_strings(
    checker: &mut Checker,
    call: &ast::ExprCall,
    summary: &FormatSummary,
    template: &Expr,
) {
    if summary.has_nested_parts {
        return;
    }

    let Expr::Attribute(ast::ExprAttribute { value, .. }) = call.func.as_ref() else {
        return;
    };

    if !value.is_string_literal_expr() {
        return;
    };

    let Some(mut summary) = FormatSummaryValues::try_from_call(call, checker.locator()) else {
        return;
    };

    let mut patches: Vec<(TextRange, String)> = vec![];
    let mut lex = lexer::lex_starts_at(
        checker.locator().slice(call.func.range()),
        Mode::Expression,
        call.start(),
    )
    .flatten();
    let end = loop {
        match lex.next() {
            Some((Tok::Dot, range)) => {
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
                break range.start();
            }
            Some((Tok::String { .. }, range)) => {
                match try_convert_to_f_string(range, &mut summary, checker.locator()) {
                    Ok(Some(fstring)) => patches.push((range, fstring)),
                    // Convert escaped curly brackets e.g. `{{` to `{` in literal string parts
                    Ok(None) => patches.push((
                        range,
                        curly_unescape(checker.locator().slice(range)).to_string(),
                    )),
                    // If any of the segments fail to convert, then we can't convert the entire
                    // expression.
                    Err(_) => return,
                }
            }
            Some(_) => continue,
            None => unreachable!("Should break from the `Tok::Dot` arm"),
        }
    };
    if patches.is_empty() {
        return;
    }

    let mut contents = String::with_capacity(checker.locator().slice(call).len());
    let mut prev_end = call.start();
    for (range, fstring) in patches {
        contents.push_str(
            checker
                .locator()
                .slice(TextRange::new(prev_end, range.start())),
        );
        contents.push_str(&fstring);
        prev_end = range.end();
    }

    // If the remainder is non-empty, add it to the contents.
    let rest = checker.locator().slice(TextRange::new(prev_end, end));
    if !lexer::lex_starts_at(rest, Mode::Expression, prev_end)
        .flatten()
        .all(|(token, _)| match token {
            Tok::Comment(_) | Tok::Newline | Tok::NonLogicalNewline | Tok::Indent | Tok::Dedent => {
                true
            }
            Tok::String { value, .. } => value.is_empty(),
            _ => false,
        })
    {
        contents.push_str(rest);
    }

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

    // Avoid refactors that exceed the line length limit.
    if !fits_or_shrinks(
        &contents,
        template.into(),
        checker.locator(),
        checker.settings.pycodestyle.max_line_length,
        checker.settings.tab_size,
    ) {
        return;
    }

    // Finally, avoid refactors that would introduce a runtime error.
    // For example, Django's `gettext` supports `format`-style arguments, but not f-strings.
    // See: https://docs.djangoproject.com/en/4.2/topics/i18n/translation
    if checker.semantic().current_expressions().any(|expr| {
        expr.as_call_expr().is_some_and(|call| {
            checker
                .semantic()
                .resolve_call_path(call.func.as_ref())
                .map_or(false, |call_path| {
                    matches!(
                        call_path.as_slice(),
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
    if !checker
        .indexer()
        .comment_ranges()
        .intersects(call.arguments.range())
    {
        diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
            contents,
            call.range(),
        )));
    };
    checker.diagnostics.push(diagnostic);
}
