use flake8_quotes::helpers::{contains_escaped_quote, raw_contents, unescape_string};
use flake8_quotes::settings::Quote;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::visitor::{Visitor, walk_f_string, walk_t_string};
use ruff_python_ast::{self as ast, AnyStringFlags, PythonVersion, StringFlags, StringLike};
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::rules::flake8_quotes;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for strings that include escaped quotes, and suggests changing
/// the quote style to avoid the need to escape them.
///
/// ## Why is this bad?
/// It's preferable to avoid escaped quotes in strings. By changing the
/// outer quote style, you can avoid escaping inner quotes.
///
/// ## Example
/// ```python
/// foo = "bar\"s"
/// ```
///
/// Use instead:
/// ```python
/// foo = 'bar"s'
/// ```
///
/// ## Formatter compatibility
/// We recommend against using this rule alongside the [formatter]. The
/// formatter automatically removes unnecessary escapes, making the rule
/// redundant.
///
/// [formatter]: https://docs.astral.sh/ruff/formatter
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.88")]
pub(crate) struct AvoidableEscapedQuote;

impl AlwaysFixableViolation for AvoidableEscapedQuote {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }

    fn fix_title(&self) -> String {
        "Change outer quotes to avoid escaping inner quotes".to_string()
    }
}

/// Q003
pub(crate) fn avoidable_escaped_quote(checker: &Checker, string_like: StringLike) {
    if checker.semantic().in_pep_257_docstring()
        || checker.semantic().in_string_type_definition()
        // This rule has support for strings nested inside another f-strings but they're checked
        // via the outermost f-string. This means that we shouldn't be checking any nested string
        // or f-string.
        || checker.semantic().in_interpolated_string_replacement_field()
    {
        return;
    }

    let mut rule_checker = AvoidableEscapedQuoteChecker::new(checker, checker.target_version());

    for part in string_like.parts() {
        match part {
            ast::StringLikePart::String(string_literal) => {
                rule_checker.visit_string_literal(string_literal);
            }
            ast::StringLikePart::Bytes(bytes_literal) => {
                rule_checker.visit_bytes_literal(bytes_literal);
            }
            ast::StringLikePart::FString(f_string) => rule_checker.visit_f_string(f_string),
            ast::StringLikePart::TString(t_string) => rule_checker.visit_t_string(t_string),
        }
    }
}

/// Checks for `Q003` violations using the [`Visitor`] implementation.
struct AvoidableEscapedQuoteChecker<'a, 'b> {
    checker: &'a Checker<'b>,
    quotes_settings: &'a flake8_quotes::settings::Settings,
    supports_pep701: bool,
}

impl<'a, 'b> AvoidableEscapedQuoteChecker<'a, 'b> {
    fn new(checker: &'a Checker<'b>, target_version: PythonVersion) -> Self {
        Self {
            checker,
            quotes_settings: &checker.settings().flake8_quotes,
            supports_pep701: target_version.supports_pep_701(),
        }
    }
}

impl Visitor<'_> for AvoidableEscapedQuoteChecker<'_, '_> {
    fn visit_string_literal(&mut self, string_literal: &ast::StringLiteral) {
        check_string_or_bytes(
            self.checker,
            self.quotes_settings,
            string_literal.range(),
            AnyStringFlags::from(string_literal.flags),
        );
    }

    fn visit_bytes_literal(&mut self, bytes_literal: &ast::BytesLiteral) {
        check_string_or_bytes(
            self.checker,
            self.quotes_settings,
            bytes_literal.range(),
            AnyStringFlags::from(bytes_literal.flags),
        );
    }

    fn visit_f_string(&mut self, f_string: &'_ ast::FString) {
        // If the target version doesn't support PEP 701, skip this entire f-string if it contains
        // any string literal in any of the expression element. For example:
        //
        // ```python
        // f"\"foo\" {'nested'}"
        // ```
        //
        // If we try to fix the above example, the outer and inner quote will be the same which is
        // invalid for any Python version before 3.12:
        //
        // ```python
        // f'"foo" {'nested'}"
        // ```
        //
        // Note that this check needs to be done globally to ignore the entire f-string. It is
        // implicitly global in that we avoid recursing into this f-string if this is the case.
        if !self.supports_pep701 {
            let contains_any_string = {
                let mut visitor = ContainsAnyString::default();
                // We need to use the `walk_f_string` instead of `visit_f_string` to avoid
                // considering the top level f-string.
                walk_f_string(&mut visitor, f_string);
                visitor.result
            };
            if contains_any_string {
                return;
            }
        }

        let opposite_quote_char = self.quotes_settings.inline_quotes.opposite().as_char();

        // If any literal part of this f-string contains the quote character which is opposite to
        // the configured inline quotes, we can't change the quote style for this f-string. For
        // example:
        //
        // ```py
        // f"\"hello\" {x} 'world'"
        // ```
        //
        // If we try to fix the above example, the f-string will end in the middle and "world" will
        // be considered as a variable which is outside this f-string:
        //
        // ```py
        // f'"hello" {x} 'world''
        // #             ^
        // #             f-string ends here now
        // ```
        //
        // The check is local to this f-string and it shouldn't check for any literal parts of any
        // nested f-string. This is correct because by this point, we know that the target version
        // is 3.12 or that this f-string doesn't have any strings nested in it. For example:
        //
        // ```py
        // f'\'normal\' {f'\'nested\' {x} "double quotes"'} normal'
        // ```
        //
        // This contains a nested f-string but if we reached here that means the target version
        // supports PEP 701. The double quotes in the nested f-string shouldn't affect the outer
        // f-string because the following is valid for Python version 3.12 and later:
        //
        // ```py
        // f"'normal' {f'\'nested\' {x} "double quotes"'} normal"
        // ```
        if !f_string
            .elements
            .literals()
            .any(|literal| contains_quote(literal, opposite_quote_char))
        {
            check_interpolated_string(
                self.checker,
                self.quotes_settings,
                AnyStringFlags::from(f_string.flags),
                &f_string.elements,
                f_string.range,
            );
        }

        walk_f_string(self, f_string);
    }

    fn visit_t_string(&mut self, t_string: &'_ ast::TString) {
        let opposite_quote_char = self.quotes_settings.inline_quotes.opposite().as_char();

        // If any literal part of this t-string contains the quote character which is opposite to
        // the configured inline quotes, we can't change the quote style for this t-string. For
        // example:
        //
        // ```py
        // t"\"hello\" {x} 'world'"
        // ```
        //
        // If we try to fix the above example, the t-string will end in the middle and "world" will
        // be considered as a variable which is outside this t-string:
        //
        // ```py
        // t'"hello" {x} 'world''
        // #             ^
        // #             t-string ends here now
        // ```
        //
        // The check is local to this t-string and it shouldn't check for any literal parts of any
        // nested t-string.
        if !t_string
            .elements
            .literals()
            .any(|literal| contains_quote(literal, opposite_quote_char))
        {
            check_interpolated_string(
                self.checker,
                self.quotes_settings,
                AnyStringFlags::from(t_string.flags),
                &t_string.elements,
                t_string.range,
            );
        }

        walk_t_string(self, t_string);
    }
}

/// Checks for unnecessary escaped quotes in a string or bytes literal.
///
/// # Panics
///
/// If the string kind is an f-string or a t-string.
fn check_string_or_bytes(
    checker: &Checker,
    quotes_settings: &flake8_quotes::settings::Settings,
    range: TextRange,
    flags: AnyStringFlags,
) {
    assert!(!flags.is_interpolated_string());

    let locator = checker.locator();

    if flags.is_triple_quoted() || flags.is_raw_string() {
        return;
    }

    // Check if we're using the preferred quotation style.
    if Quote::from(flags.quote_style()) != quotes_settings.inline_quotes {
        return;
    }

    let contents = raw_contents(locator.slice(range), flags);

    if !contains_escaped_quote(contents, quotes_settings.inline_quotes.as_char())
        || contains_quote(contents, quotes_settings.inline_quotes.opposite().as_char())
    {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(AvoidableEscapedQuote, range);
    let fixed_contents = format!(
        "{prefix}{quote}{value}{quote}",
        prefix = flags.prefix(),
        quote = quotes_settings.inline_quotes.opposite().as_char(),
        value = unescape_string(contents, quotes_settings.inline_quotes.as_char())
    );
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        fixed_contents,
        range,
    )));
}

/// Checks for unnecessary escaped quotes in an f-string or t-string.
fn check_interpolated_string(
    checker: &Checker,
    quotes_settings: &flake8_quotes::settings::Settings,
    flags: ast::AnyStringFlags,
    elements: &ast::InterpolatedStringElements,
    range: TextRange,
) {
    if flags.is_triple_quoted() || flags.prefix().is_raw() {
        return;
    }

    // Check if we're using the preferred quotation style.
    if Quote::from(flags.quote_style()) != quotes_settings.inline_quotes {
        return;
    }

    let quote_char = quotes_settings.inline_quotes.as_char();
    let opposite_quote_char = quotes_settings.inline_quotes.opposite().as_char();

    let mut edits = vec![];
    for literal in elements.literals() {
        let content = checker.locator().slice(literal);
        if !contains_escaped_quote(content, quote_char) {
            continue;
        }
        edits.push(Edit::range_replacement(
            unescape_string(content, quote_char),
            literal.range(),
        ));
    }

    if edits.is_empty() {
        return;
    }

    // Replacement for the f/t-string opening quote. We don't perform the check for raw and
    // triple-quoted f-strings, so no need to account for them.
    let start_edit = Edit::range_replacement(
        format!("{}{opposite_quote_char}", flags.prefix()),
        TextRange::at(
            range.start(),
            // Prefix + quote char
            TextSize::new(2),
        ),
    );

    // Replacement for the f/t-string ending quote. We don't perform the check for triple-quoted
    // f-string, so no need to account for them.
    edits.push(Edit::range_replacement(
        opposite_quote_char.to_string(),
        TextRange::at(
            // Offset would either be the end offset of the start edit in case there are no
            // elements in the f/t-string (e.g., `f""`) or the end offset of the last f/t-string
            // element (e.g., `f"hello"`).
            elements
                .last()
                .map_or_else(|| start_edit.end(), Ranged::end),
            // Quote char
            TextSize::new(1),
        ),
    ));

    checker
        .report_diagnostic(AvoidableEscapedQuote, range)
        .set_fix(Fix::safe_edits(start_edit, edits));
}

#[derive(Debug, Default)]
struct ContainsAnyString {
    result: bool,
}

impl Visitor<'_> for ContainsAnyString {
    fn visit_string_literal(&mut self, _: &'_ ast::StringLiteral) {
        self.result = true;
    }

    fn visit_bytes_literal(&mut self, _: &'_ ast::BytesLiteral) {
        self.result = true;
    }

    fn visit_f_string(&mut self, _: &'_ ast::FString) {
        self.result = true;
        // We don't need to recurse into this f-string now that we already know the result.
    }

    fn visit_t_string(&mut self, _: &'_ ast::TString) {
        self.result = true;
        // We don't need to recurse into this t-string now that we already know the result.
    }
}

/// Return `true` if the haystack contains the quote.
fn contains_quote(haystack: &str, quote: char) -> bool {
    memchr::memchr(quote as u8, haystack.as_bytes()).is_some()
}
