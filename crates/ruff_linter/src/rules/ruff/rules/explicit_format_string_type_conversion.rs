use std::str::FromStr;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::str_prefix::StringLiteralPrefix;
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_literal::cformat::{
    CFormatPart, CFormatPrecision, CFormatQuantity, CFormatSpec, CFormatString,
};
use ruff_python_literal::format::{
    FieldName, FieldType, FormatPart, FormatString, FromTemplate as _,
};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for uses of `str()`, `repr()`, and `ascii()` as explicit type conversions
/// within `%`-style format strings and `str.format` calls.
///
/// ## Why is this bad?
/// Both formatting styles support dedicated conversion flags for these types, which are
/// more succinct and idiomatic: `%r` and `!r` for `repr()`, `%a` and `!a` for `ascii()`,
/// and `%s` and `!s` for `str()`.
///
/// A `%s` or `!r` conversion already converts its value with `str()`, so an explicit `str()` call
/// is redundant there and can be dropped entirely in many cases. The notable exception being for
/// classes that implement a custom `__format__` method.
///
/// f-strings are covered by [`explicit-f-string-type-conversion`][RUF010] instead.
///
/// ## Example
/// ```python
/// "%s %s %s" % (repr(foo), str(bar), baz)
/// "{} {} {}".format(repr(foo), str(bar), baz)
/// ```
///
/// Use instead:
/// ```python
/// "%r %s %s" % (foo, bar, baz)
/// "{!r} {!s} {}".format(foo, bar, baz)
/// ```
///
/// ## Known problems
///
/// This rule is unnecessary if [`printf-string-formatting`][UP031] and [`f-string`][UP032] are
/// enabled alongside [`explicit-f-string-type-conversion`][RUF010]: those rules first rewrite
/// `"%s" % (repr(foo),)` and `"{}".format(repr(foo))` into `f"{repr(foo)}"`, which `RUF010`
/// then rewrites into `f"{foo!r}"`. Where the fixes overlap, only one is applied per pass, so
/// the outcome is the same either way, but two diagnostics are reported instead of one.
///
/// ## Fix safety
///
/// This rule's fix is marked as unsafe if the conversion call contains comments that would
/// be deleted by applying the fix.
///
/// It is also marked as unsafe when a `%`-format mapping writes the same key more than once,
/// as in `"%(value)s" % {"value": repr(value), "value": repr(value)}`. Only the last entry
/// survives, so every entry has to be rewritten together, and a duplicated key is usually a
/// mistake worth looking at.
///
/// No fix is offered when a `%`-format mapping could supply the key from somewhere this rule
/// can't inspect: entries written with the same key that don't agree
/// (`{"value": repr(value), "value": str(value)}`), or an entry that could be overwritten by
/// a later key that isn't a literal (`{"value": repr(value), **overrides}`).
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
/// - [Python documentation: Format String Syntax](https://docs.python.org/3/library/string.html#format-string-syntax)
/// - [`explicit-f-string-type-conversion` (`RUF010`)][RUF010]
/// - [`printf-string-formatting` (`UP031`)][UP031]
/// - [`f-string` (`UP032`)][UP032]
///
/// [RUF010]: https://docs.astral.sh/ruff/rules/explicit-f-string-type-conversion/
/// [UP031]: https://docs.astral.sh/ruff/rules/printf-string-formatting/
/// [UP032]: https://docs.astral.sh/ruff/rules/f-string/
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct ExplicitFormatStringTypeConversion {
    conversion: Conversion,
    style: FormatStyle,
}

impl Violation for ExplicitFormatStringTypeConversion {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let ExplicitFormatStringTypeConversion { conversion, style } = self;
        let function = conversion.function();
        let flag = conversion.flag();
        match (style, conversion) {
            (FormatStyle::Percent, Conversion::Str) => {
                format!("Unnecessary `{function}()` call within a `%s` conversion")
            }
            (FormatStyle::Percent, _) => {
                format!("Use `%{flag}` instead of calling `{function}()`")
            }
            (FormatStyle::Format, _) => {
                format!("Use the `!{flag}` conversion flag instead of calling `{function}()`")
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let ExplicitFormatStringTypeConversion { conversion, style } = self;
        let function = conversion.function();
        let flag = conversion.flag();
        Some(match (style, conversion) {
            (FormatStyle::Percent, Conversion::Str) => format!("Remove `{function}()` call"),
            (FormatStyle::Percent, _) => format!("Replace with `%{flag}`"),
            (FormatStyle::Format, _) => format!("Replace with `!{flag}` conversion flag"),
        })
    }
}

/// RUF077
pub(crate) fn percent_format_type_conversion(
    checker: &Checker,
    bin_op: &ast::ExprBinOp,
    format_string: &ast::ExprStringLiteral,
) {
    // Parsing the format string is comparatively expensive, and the vast majority of
    // `%`-formatting has nothing to rewrite, so check the values first.
    let values = bin_op.right.as_ref();
    let any_candidate = match values {
        Expr::Tuple(tuple) => tuple.iter().any(|value| is_conversion_call(checker, value)),
        Expr::Dict(dict) => dict
            .iter_values()
            .any(|value| is_conversion_call(checker, value)),
        value => is_conversion_call(checker, value),
    };
    if !any_candidate {
        return;
    }

    let Some(conversions) = percent_conversions(checker, format_string) else {
        return;
    };

    match values {
        // Ex) `"%(value)s" % {"value": repr(value)}`
        Expr::Dict(dict)
            if conversions
                .iter()
                .all(|conversion| conversion.key.is_some()) =>
        {
            percent_mapping(checker, &conversions, dict);
        }

        // Ex) `"%s %s" % (repr(first), second)`
        Expr::Tuple(tuple) => {
            // Ex) `"%s %s" % (*values,)`: we can't tell which conversion a value belongs to.
            if tuple.iter().any(Expr::is_starred_expr) {
                return;
            }
            let Some(pairs) = zip_percent_values(&conversions, tuple) else {
                return;
            };
            for (format, value) in pairs {
                report_percent(
                    checker,
                    value,
                    &[format],
                    PercentValues::Element,
                    Fixability::Allowed,
                );
            }
        }

        // Ex) `"%s" % repr(value)`. The right-hand side is a single value rather than a
        // one-element tuple, so the fix has to introduce the tuple itself.
        value => {
            let Some(pairs) = zip_percent_values(&conversions, std::iter::once(value)) else {
                return;
            };
            for (format, value) in pairs {
                report_percent(
                    checker,
                    value,
                    &[format],
                    PercentValues::Single,
                    Fixability::Allowed,
                );
            }
        }
    }
}

/// Report the values of a `%`-format mapping, as in `"%(value)s" % {"value": repr(value)}`.
///
/// Unlike a tuple, a dictionary display doesn't pair each conversion with exactly one entry:
/// a key can be written more than once, and an entry whose key isn't a literal (including a
/// `**` unpacking) can supply any key at all. In both cases the conversion may end up applied
/// to a value other than the one being rewritten, so the diagnostic is still reported but the
/// fix is withheld or downgraded.
fn percent_mapping(checker: &Checker, conversions: &[PercentConversion], dict: &ast::ExprDict) {
    // Later entries overwrite earlier ones, so only an entry that isn't followed by a key we
    // can't evaluate is safe to rewrite. Ex) `{**base, "value": repr(value)}` is fine, but
    // `{"value": repr(value), **base}` is not.
    let last_opaque_key = dict
        .iter()
        .rposition(|item| !matches!(item.key.as_ref(), Some(Expr::StringLiteral(_))));

    for (index, item) in dict.iter().enumerate() {
        let Some(Expr::StringLiteral(key)) = item.key.as_ref() else {
            continue;
        };
        let key = key.value.to_str();

        let formats: Vec<&PercentConversion> = conversions
            .iter()
            .filter(|conversion| conversion.key.as_deref() == Some(key))
            .collect();

        let fixability = if last_opaque_key.is_some_and(|opaque| opaque > index) {
            Fixability::Disallowed
        } else {
            match key_entries(checker, dict, key) {
                KeyEntries::Unique => Fixability::Allowed,
                // Every entry for the key would be rewritten the same way, so the result is
                // unchanged. A duplicated key is a mistake often enough that the rewrite is
                // still worth a second look, so the fix is offered as unsafe.
                KeyEntries::DuplicatedAndEqual => Fixability::Unsafe,
                KeyEntries::DuplicatedAndDifferent => Fixability::Disallowed,
            }
        };

        report_percent(
            checker,
            &item.value,
            &formats,
            PercentValues::Element,
            fixability,
        );
    }
}

/// How the entries of `dict` that are written with `key` relate to one another.
#[derive(Debug, Copy, Clone)]
enum KeyEntries {
    /// Exactly one entry uses the key, so it is the value the conversion formats.
    Unique,
    /// Several entries use the key, and they all apply the same conversion to the same
    /// expression.
    DuplicatedAndEqual,
    /// Several entries use the key, and they differ, so rewriting them isn't equivalent.
    DuplicatedAndDifferent,
}

/// Classify the entries of `dict` written with `key`.
///
/// This is quadratic in the number of entries when called for each of them, but the
/// dictionaries written inline as the right-hand side of a `%` are small, and this only runs
/// once a conversion call has already been found in one of them.
fn key_entries(checker: &Checker, dict: &ast::ExprDict, key: &str) -> KeyEntries {
    let mut count = 0usize;
    let mut equal = true;
    let mut expected = None;

    for item in dict {
        let Some(Expr::StringLiteral(item_key)) = item.key.as_ref() else {
            continue;
        };
        if item_key.value.to_str() != key {
            continue;
        }

        count += 1;
        let current = conversion_call(checker, &item.value)
            .map(|(conversion, _, argument)| (conversion, ComparableExpr::from(argument)));
        if count == 1 {
            expected = current;
        } else {
            equal &= current.is_some() && current == expected;
        }
    }

    match count {
        0 | 1 => KeyEntries::Unique,
        _ if equal => KeyEntries::DuplicatedAndEqual,
        _ => KeyEntries::DuplicatedAndDifferent,
    }
}

/// RUF077
pub(crate) fn format_call_type_conversion(
    checker: &Checker,
    call: &ast::ExprCall,
    format_string: &ast::ExprStringLiteral,
) {
    // Ex) `"{} {}".format(*values)`: we can't tell which field a value belongs to.
    if call.arguments.args.iter().any(Expr::is_starred_expr) {
        return;
    }

    // Parsing the format string is comparatively expensive, and the vast majority of
    // `str.format` calls have nothing to rewrite, so check the arguments first.
    let any_candidate = call
        .arguments
        .args
        .iter()
        .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
        .any(|value| is_conversion_call(checker, value));
    if !any_candidate {
        return;
    }

    let Some(fields) = format_fields(checker, format_string) else {
        return;
    };

    let mut replacements = Vec::with_capacity(fields.len());
    let mut automatic = 0usize;
    let mut has_automatic = false;
    let mut has_explicit = false;
    for field in &fields {
        // `FieldName::parse` validates the whole name, rejecting the likes of `{value[0}`,
        // and reports whether it reaches into the argument. The name of the argument itself
        // is taken from the source slice instead, so that matching it against the call's
        // keywords doesn't allocate.
        let Ok(name) = FieldName::parse(field.name) else {
            return;
        };
        let reference = match name.field_type {
            FieldType::Auto => {
                has_automatic = true;
                let index = automatic;
                automatic += 1;
                Reference::Positional(index)
            }
            FieldType::Index(index) => {
                has_explicit = true;
                Reference::Positional(index)
            }
            FieldType::Keyword(_) => Reference::Keyword(argument_name(field.name)),
        };
        replacements.push(Replacement {
            field,
            reference,
            direct: name.parts.is_empty(),
        });
    }

    // Ex) `"{} {0}".format(first, second)`: Python rejects mixing automatic and explicit
    // field numbering.
    if has_automatic && has_explicit {
        return;
    }

    for (index, value) in call.arguments.args.iter().enumerate() {
        report_format(checker, value, &replacements, &Reference::Positional(index));
    }
    // A `**` unpacking needs no special handling here, unlike in a `%`-format mapping: an
    // argument it duplicates is a `TypeError` rather than a silent overwrite, and a field it
    // supplies matches no explicit keyword, so nothing is rewritten on its behalf.
    for keyword in &call.arguments.keywords {
        let Some(name) = keyword.arg.as_ref() else {
            continue;
        };
        report_format(
            checker,
            &keyword.value,
            &replacements,
            &Reference::Keyword(name.as_str()),
        );
    }
}

/// The part of a field name that names an argument, i.e. everything before any attribute
/// access or subscript. Ex) the `value` of `{value.attr[0]}`.
fn argument_name(name: &str) -> &str {
    name.split(['.', '[']).next().unwrap_or(name)
}

/// How the values of a `%`-format expression are written, which determines what a value has
/// to be replaced with once its conversion call is removed.
#[derive(Debug, Copy, Clone)]
enum PercentValues {
    /// The value is one element of a tuple or of a mapping, as in `"%s" % (value,)` and
    /// `"%(key)s" % {"key": value}`.
    Element,
    /// The value is the entire right-hand side, as in `"%s" % value`.
    Single,
}

/// Whether the conversion call of a reported value may be rewritten.
#[derive(Debug, Copy, Clone)]
enum Fixability {
    /// Offer the fix, unsafe only if it would delete comments.
    Allowed,
    /// Offer the fix, but always as unsafe.
    Unsafe,
    /// Report the diagnostic without a fix.
    Disallowed,
}

/// Report and fix a single value of a `%`-format expression, if it is an explicit conversion
/// call and every conversion that formats it is a `%s`.
fn report_percent(
    checker: &Checker,
    value: &Expr,
    formats: &[&PercentConversion],
    values: PercentValues,
    fixability: Fixability,
) {
    let Some((conversion, call, argument)) = conversion_call(checker, value) else {
        return;
    };

    // A conversion other than `%s` already applies a conversion of its own, so swapping in
    // `%r` or `%a` would change the result. `formats` is empty when a mapping holds a key
    // that the format string never refers to, in which case the value is simply unused.
    if formats.is_empty() || formats.iter().any(|format| format.character != 's') {
        return;
    }

    let applicability = match fixability {
        Fixability::Allowed => applicability(checker, call, argument),
        Fixability::Unsafe => Applicability::Unsafe,
        Fixability::Disallowed => {
            checker.report_diagnostic(
                ExplicitFormatStringTypeConversion {
                    conversion,
                    style: FormatStyle::Percent,
                },
                call.range(),
            );
            return;
        }
    };

    let mut edits: Vec<Edit> = Vec::with_capacity(formats.len());
    // `%s` already converts its value with `str()`, so only `repr()` and `ascii()` need the
    // format string itself to change.
    if conversion != Conversion::Str {
        edits.extend(formats.iter().map(|format| {
            Edit::range_replacement(
                conversion.flag().to_string(),
                TextRange::at(format.offset, 's'.text_len()),
            )
        }));
    }

    let replacement = match values {
        PercentValues::Element => argument_source(checker, call, argument),
        // `"%s" % value` treats a tuple or a mapping on the right-hand side as several values
        // rather than as one, so the value has to be wrapped in a one-element tuple to keep
        // its meaning. That is only equivalent because the conversion call we are removing
        // always returns a `str`, which `%`-formatting treats as a single value.
        PercentValues::Single => format!("({},)", argument_source(checker, call, argument)),
    };

    let mut diagnostic = checker.report_diagnostic(
        ExplicitFormatStringTypeConversion {
            conversion,
            style: FormatStyle::Percent,
        },
        call.range(),
    );
    diagnostic.set_fix(Fix::applicable_edits(
        Edit::range_replacement(replacement, call.range()),
        edits,
        applicability,
    ));
}

/// Report and fix a single argument of a `str.format` call, if it is an explicit conversion
/// call and every field that formats it can take a conversion flag.
fn report_format(
    checker: &Checker,
    value: &Expr,
    replacements: &[Replacement],
    reference: &Reference,
) {
    let Some((conversion, call, argument)) = conversion_call(checker, value) else {
        return;
    };

    let fields = replacements
        .iter()
        .filter(|replacement| replacement.reference == *reference);

    let mut edits: Vec<Edit> = Vec::new();
    for Replacement { field, direct, .. } in fields {
        // Ex) `"{!s}".format(repr(value))` already converts, and `"{.attr}".format(repr(value))`
        // formats an attribute of the conversion's result rather than the value itself.
        if field.has_conversion || !direct {
            return;
        }
        edits.push(Edit::insertion(
            format!("!{}", conversion.flag()),
            field.conversion_offset,
        ));
    }

    // The argument isn't referenced by any field, so it is simply unused.
    if edits.is_empty() {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        ExplicitFormatStringTypeConversion {
            conversion,
            style: FormatStyle::Format,
        },
        call.range(),
    );
    diagnostic.set_fix(Fix::applicable_edits(
        Edit::range_replacement(argument_source(checker, call, argument), call.range()),
        edits,
        applicability(checker, call, argument),
    ));
}

/// A `%` conversion, located within the source text of a format string.
#[derive(Debug)]
struct PercentConversion {
    /// The mapping key, as in the `key` of `%(key)s`.
    key: Option<String>,
    /// The conversion character, as in the `s` of `%s`.
    character: char,
    /// How many values the width and the precision consume, as in `"%*.*s" % (10, 4, value)`.
    starred: usize,
    /// The offset of the conversion character in the source.
    offset: TextSize,
}

/// Locate every `%` conversion in the source text of `format_string`.
///
/// Returns `None` if the format string can't be mapped onto its source text.
fn percent_conversions(
    checker: &Checker,
    format_string: &ast::ExprStringLiteral,
) -> Option<Vec<PercentConversion>> {
    let parsed = parse_percent_parts(checker, format_string)?;

    let mut conversions = Vec::new();
    let mut specs: Vec<&CFormatSpec> = Vec::new();
    for (part, parsed) in format_string.value.iter().zip(&parsed) {
        let content_range = part.content_range();
        let content = checker.locator().slice(content_range);
        for (spec, offset) in conversion_characters(parsed, content)? {
            conversions.push(PercentConversion {
                key: spec.mapping_key.clone(),
                character: spec.format_char,
                starred: usize::from(matches!(
                    spec.min_field_width,
                    Some(CFormatQuantity::FromValuesTuple)
                )) + usize::from(matches!(
                    spec.precision,
                    Some(CFormatPrecision::Quantity(CFormatQuantity::FromValuesTuple))
                )),
                offset: content_range.start() + offset,
            });
            specs.push(spec);
        }
    }

    // `%`-formatting operates on the *value* of the string literal, but a fix has to edit its
    // source text, so parse both and require that they agree. They can differ when an escape
    // sequence expands to a `%` (as in `"\x25s"`), or when a conversion is split across
    // implicitly concatenated parts (as in `"%" "s"`).
    let value = CFormatString::from_str(format_string.value.to_str()).ok()?;
    let mut expected = value.iter().filter_map(|(_, part)| match part {
        CFormatPart::Spec(spec) => Some(spec),
        CFormatPart::Literal(_) => None,
    });
    for spec in specs {
        if expected.next() != Some(spec) {
            return None;
        }
    }
    if expected.next().is_some() {
        return None;
    }

    Some(conversions)
}

/// Parse the source text of each part of `format_string` as a `%`-format string.
fn parse_percent_parts(
    checker: &Checker,
    format_string: &ast::ExprStringLiteral,
) -> Option<Vec<CFormatString>> {
    format_string
        .value
        .iter()
        .map(|part| CFormatString::from_str(checker.locator().slice(part.content_range())).ok())
        .collect()
}

/// Locate the conversion character of every `%` conversion in `content`.
///
/// [`CFormatString`] records the character index at which each part starts, so a conversion
/// ends where the following part begins, and its conversion character is the last character
/// before that.
fn conversion_characters<'a>(
    parsed: &'a CFormatString,
    content: &str,
) -> Option<Vec<(&'a CFormatSpec, TextSize)>> {
    // The byte offset of every character in `content`, plus the offset just past its end.
    let mut offsets: Vec<TextSize> = Vec::with_capacity(content.len() + 1);
    let mut offset = TextSize::default();
    for character in content.chars() {
        offsets.push(offset);
        offset += character.text_len();
    }
    offsets.push(offset);

    let mut conversions = Vec::new();
    let mut parts = parsed.iter().peekable();
    while let Some((_, part)) = parts.next() {
        let CFormatPart::Spec(spec) = part else {
            continue;
        };
        let end = parts.peek().map_or(offsets.len() - 1, |(next, _)| *next);
        conversions.push((spec, *offsets.get(end.checked_sub(1)?)?));
    }
    Some(conversions)
}

/// Pair each `%` conversion with the value it formats.
///
/// Returns `None` if the values don't line up with the conversions, which is a `TypeError` at
/// runtime.
fn zip_percent_values<'a, 'b>(
    conversions: &'b [PercentConversion],
    values: impl IntoIterator<Item = &'a Expr>,
) -> Option<Vec<(&'b PercentConversion, &'a Expr)>> {
    let mut values = values.into_iter();
    let mut pairs = Vec::with_capacity(conversions.len());
    for conversion in conversions {
        // A `*` in the width or the precision consumes a value of its own, as in
        // `"%*.*s" % (10, 4, value)`.
        for _ in 0..conversion.starred {
            values.next()?;
        }
        pairs.push((conversion, values.next()?));
    }
    if values.next().is_some() {
        return None;
    }
    Some(pairs)
}

/// A `{...}` replacement field, located within the source text of a `str.format` template.
#[derive(Debug)]
struct Field<'a> {
    /// The name of the field, i.e. everything before its conversion and format spec.
    name: &'a str,
    /// Whether the field already specifies a conversion, as in `{value!r}`.
    has_conversion: bool,
    /// Where a conversion flag would have to be inserted, i.e. just before the `!` or `:`
    /// that ends the field name, or before the closing `}`.
    conversion_offset: TextSize,
}

/// A [`Field`] resolved to the argument of the `str.format` call that it refers to.
#[derive(Debug)]
struct Replacement<'a> {
    field: &'a Field<'a>,
    reference: Reference<'a>,
    /// Whether the field formats the argument itself, rather than an attribute or an item of
    /// it (as in `{value.attr}` or `{value[0]}`).
    direct: bool,
}

/// The argument that a replacement field refers to.
#[derive(Debug, PartialEq, Eq)]
enum Reference<'a> {
    Positional(usize),
    Keyword(&'a str),
}

/// Locate every replacement field in the source text of `format_string`.
///
/// Returns `None` if the format string can't be mapped onto its source text.
fn format_fields<'a>(
    checker: &Checker<'a>,
    format_string: &ast::ExprStringLiteral,
) -> Option<Vec<Field<'a>>> {
    // `str.format` operates on the *value* of the string literal, but a fix has to edit its
    // source text, so parse both and require that they agree. They can differ when an escape
    // sequence expands to a brace (as in `"\x7b}"`), or when a field is split across
    // implicitly concatenated parts (as in `"{" "}"`).
    let mut fields = Vec::new();
    for part in &format_string.value {
        let content_range = part.content_range();
        let raw = matches!(part.flags.prefix(), StringLiteralPrefix::Raw { .. });
        let content = checker.locator().slice(content_range);
        for mut field in parse_fields(content, raw)? {
            field.conversion_offset += content_range.start();
            fields.push(field);
        }
    }

    let value = FormatString::from_raw_str(format_string.value.to_str()).ok()?;
    let mut expected = value.format_parts.iter().filter_map(|part| match part {
        FormatPart::Field {
            field_name,
            conversion_spec,
            format_spec,
        } => Some((field_name, conversion_spec, format_spec)),
        FormatPart::Literal(_) => None,
    });
    for field in &fields {
        let (field_name, conversion_spec, format_spec) = expected.next()?;
        if *field_name != field.name || conversion_spec.is_some() != field.has_conversion {
            return None;
        }
        // Ex) `"{:>{width}}"`: a nested field consumes an argument of its own, which we make
        // no attempt to track.
        if format_spec.contains('{') {
            return None;
        }
    }
    if expected.next().is_some() {
        return None;
    }

    Some(fields)
}

/// Locate every replacement field in `content`, the source text of one part of a `str.format`
/// template.
///
/// Returns `None` if `content` is malformed, or if it contains a nested replacement field
/// (as in `"{:>{width}}"`), which we make no attempt to rewrite.
fn parse_fields(content: &str, raw: bool) -> Option<Vec<Field<'_>>> {
    let bytes = content.as_bytes();
    let mut fields = Vec::new();
    let mut index = 0;
    // Whether the previous character was a backslash that isn't itself escaped.
    let mut pending_escape = false;

    while index < bytes.len() {
        // Ex) `"\N{BULLET}"`: a named Unicode escape, not a replacement field. The scan works
        // on bytes rather than on `str` slices so that `index` never has to sit on a character
        // boundary; every byte it tests for is ASCII, and the continuation bytes of a
        // multi-byte character can't be mistaken for one.
        if !raw && pending_escape && bytes[index..].starts_with(b"N{") {
            index += bytes[index..].iter().position(|byte| *byte == b'}')? + 1;
            pending_escape = false;
            continue;
        }

        match bytes[index] {
            // Ex) `"{{"` and `"}}"`: escaped braces.
            byte @ (b'{' | b'}') if bytes.get(index + 1) == Some(&byte) => {
                index += 2;
                pending_escape = false;
            }
            b'}' => return None,
            b'{' => {
                let mut cursor = index + 1;
                let mut name_end = None;
                loop {
                    match bytes.get(cursor)? {
                        b'{' => return None,
                        b'}' => break,
                        b'!' | b':' if name_end.is_none() => name_end = Some(cursor),
                        _ => {}
                    }
                    cursor += 1;
                }
                let name_end = name_end.unwrap_or(cursor);
                fields.push(Field {
                    name: content.get(index + 1..name_end)?,
                    has_conversion: bytes.get(name_end) == Some(&b'!'),
                    conversion_offset: TextSize::try_from(name_end).ok()?,
                });
                index = cursor + 1;
                pending_escape = false;
            }
            byte => {
                pending_escape = byte == b'\\' && !pending_escape;
                index += 1;
            }
        }
    }

    Some(fields)
}

/// The built-in conversion functions that have an equivalent conversion flag.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Conversion {
    Str,
    Repr,
    Ascii,
}

impl Conversion {
    /// The name of the built-in function.
    const fn function(self) -> &'static str {
        match self {
            Conversion::Str => "str",
            Conversion::Repr => "repr",
            Conversion::Ascii => "ascii",
        }
    }

    /// The conversion character, as used in `%r` and `{!r}`.
    const fn flag(self) -> char {
        match self {
            Conversion::Str => 's',
            Conversion::Repr => 'r',
            Conversion::Ascii => 'a',
        }
    }
}

/// Returns `true` if `expr` is a call that a conversion flag could replace.
fn is_conversion_call(checker: &Checker, expr: &Expr) -> bool {
    conversion_call(checker, expr).is_some()
}

/// If `expr` is a call to the built-in `str`, `repr`, or `ascii` with a single argument that
/// can be hoisted out of it, return the conversion, the call, and that argument.
fn conversion_call<'a>(
    checker: &Checker,
    expr: &'a Expr,
) -> Option<(Conversion, &'a ast::ExprCall, &'a Expr)> {
    let Expr::Call(call) = expr else {
        return None;
    };

    let conversion = match checker.semantic().resolve_builtin_symbol(&call.func)? {
        "str" => Conversion::Str,
        "repr" => Conversion::Repr,
        "ascii" => Conversion::Ascii,
        _ => return None,
    };

    let argument = match conversion {
        // Ex) `str(object=value)`, which `repr` and `ascii` don't accept.
        Conversion::Str if call.arguments.len() == 1 => {
            call.arguments.find_argument_value("object", 0)?
        }
        Conversion::Str | Conversion::Repr | Conversion::Ascii => {
            if !call.arguments.keywords.is_empty() {
                return None;
            }
            let [argument] = call.arguments.args.as_ref() else {
                return None;
            };
            argument
        }
    };

    if argument.is_starred_expr() {
        return None;
    }

    Some((conversion, call, argument))
}

/// The source text to replace a conversion call with.
fn argument_source(checker: &Checker, call: &ast::ExprCall, argument: &Expr) -> String {
    let source = checker
        .locator()
        .slice(argument_range(checker, call, argument));
    // A generator expression may only go unparenthesized when it is the sole argument of a
    // call, as in `repr(x for x in y)`. Once hoisted out it can end up beside other arguments,
    // as in `"{} {}".format(x for x in y, z)`, so parenthesize it.
    if argument
        .as_generator_expr()
        .is_some_and(|generator| !generator.parenthesized)
    {
        format!("({source})")
    } else {
        source.to_string()
    }
}

/// The range of `argument`, including any parentheses that wrap it within `call`.
fn argument_range(checker: &Checker, call: &ast::ExprCall, argument: &Expr) -> TextRange {
    parenthesized_range(argument.into(), (&call.arguments).into(), checker.tokens())
        .unwrap_or(argument.range())
}

/// Removing a conversion call deletes anything between the function name and its argument, so
/// the fix is only safe when there are no comments there.
fn applicability(checker: &Checker, call: &ast::ExprCall, argument: &Expr) -> Applicability {
    let argument_range = argument_range(checker, call, argument);
    let comment_ranges = checker.comment_ranges();
    if comment_ranges.intersects(TextRange::new(call.start(), argument_range.start()))
        || comment_ranges.intersects(TextRange::new(argument_range.end(), call.end()))
    {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    }
}

/// The kind of string formatting that a conversion call appears in.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum FormatStyle {
    /// Ex) `"%s" % value`
    Percent,
    /// Ex) `"{}".format(value)`
    Format,
}
