use std::string::ToString;

use rustc_hash::FxHashSet;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::Name;
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

use super::super::cformat::CFormatSummary;
use super::super::fixes::{
    remove_unused_format_arguments_from_dict, remove_unused_keyword_arguments_from_format_call,
    remove_unused_positional_arguments_from_format_call,
};
use super::super::format::FormatSummary;

/// ## What it does
/// Checks for invalid `printf`-style format strings.
///
/// ## Why is this bad?
/// Conversion specifiers are required for `printf`-style format strings. These
/// specifiers must contain a `%` character followed by a conversion type.
///
/// ## Example
/// ```python
/// "Hello, %" % "world"
/// ```
///
/// Use instead:
/// ```python
/// "Hello, %s" % "world"
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatInvalidFormat {
    pub(crate) message: String,
}

impl Violation for PercentFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatInvalidFormat { message } = self;
        format!("`%`-format string has invalid format string: {message}")
    }
}

/// ## What it does
/// Checks for named placeholders in `printf`-style format strings without
/// mapping-type values.
///
/// ## Why is this bad?
/// When using named placeholders in `printf`-style format strings, the values
/// must be a map type (such as a dictionary). Otherwise, the expression will
/// raise a `TypeError`.
///
/// ## Example
/// ```python
/// "%(greeting)s, %(name)s" % ("Hello", "World")
/// ```
///
/// Use instead:
/// ```python
/// "%(greeting)s, %(name)s" % {"greeting": "Hello", "name": "World"}
/// ```
///
/// Or:
/// ```python
/// "%s, %s" % ("Hello", "World")
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatExpectedMapping;

impl Violation for PercentFormatExpectedMapping {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`%`-format string expected mapping but got sequence".to_string()
    }
}

/// ## What it does
/// Checks for uses of mapping-type values in `printf`-style format strings
/// without named placeholders.
///
/// ## Why is this bad?
/// When using mapping-type values (such as `dict`) in `printf`-style format
/// strings, the keys must be named. Otherwise, the expression will raise a
/// `TypeError`.
///
/// ## Example
/// ```python
/// "%s, %s" % {"greeting": "Hello", "name": "World"}
/// ```
///
/// Use instead:
/// ```python
/// "%(greeting)s, %(name)s" % {"greeting": "Hello", "name": "World"}
/// ```
///
/// Or:
/// ```python
/// "%s, %s" % ("Hello", "World")
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatExpectedSequence;

impl Violation for PercentFormatExpectedSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`%`-format string expected sequence but got mapping".to_string()
    }
}

/// ## What it does
/// Checks for unused mapping keys in `printf`-style format strings.
///
/// ## Why is this bad?
/// Unused named placeholders in `printf`-style format strings are unnecessary,
/// and likely indicative of a mistake. They should be removed.
///
/// ## Example
/// ```python
/// "Hello, %(name)s" % {"greeting": "Hello", "name": "World"}
/// ```
///
/// Use instead:
/// ```python
/// "Hello, %(name)s" % {"name": "World"}
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatExtraNamedArguments {
    missing: Vec<String>,
}

impl AlwaysFixableViolation for PercentFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("`%`-format string has unused named argument(s): {message}")
    }

    fn fix_title(&self) -> String {
        let PercentFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

/// ## What it does
/// Checks for named placeholders in `printf`-style format strings that are not
/// present in the provided mapping.
///
/// ## Why is this bad?
/// Named placeholders that lack a corresponding value in the provided mapping
/// will raise a `KeyError`.
///
/// ## Example
/// ```python
/// "%(greeting)s, %(name)s" % {"name": "world"}
/// ```
///
/// Use instead:
/// ```python
/// "Hello, %(name)s" % {"name": "world"}
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatMissingArgument {
    missing: Vec<String>,
}

impl Violation for PercentFormatMissingArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatMissingArgument { missing } = self;
        let message = missing.join(", ");
        format!("`%`-format string is missing argument(s) for placeholder(s): {message}")
    }
}

/// ## What it does
/// Checks for `printf`-style format strings that have mixed positional and
/// named placeholders.
///
/// ## Why is this bad?
/// Python does not support mixing positional and named placeholders in
/// `printf`-style format strings. The use of mixed placeholders will raise a
/// `TypeError` at runtime.
///
/// ## Example
/// ```python
/// "%s, %(name)s" % ("Hello", {"name": "World"})
/// ```
///
/// Use instead:
/// ```python
/// "%s, %s" % ("Hello", "World")
/// ```
///
/// Or:
/// ```python
/// "%(greeting)s, %(name)s" % {"greeting": "Hello", "name": "World"}
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatMixedPositionalAndNamed;

impl Violation for PercentFormatMixedPositionalAndNamed {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`%`-format string has mixed positional and named placeholders".to_string()
    }
}

/// ## What it does
/// Checks for `printf`-style format strings that have a mismatch between the
/// number of positional placeholders and the number of substitution values.
///
/// ## Why is this bad?
/// When a `printf`-style format string is provided with too many or too few
/// substitution values, it will raise a `TypeError` at runtime.
///
/// ## Example
/// ```python
/// "%s, %s" % ("Hello", "world", "!")
/// ```
///
/// Use instead:
/// ```python
/// "%s, %s" % ("Hello", "world")
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatPositionalCountMismatch {
    wanted: usize,
    got: usize,
}

impl Violation for PercentFormatPositionalCountMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatPositionalCountMismatch { wanted, got } = self;
        format!("`%`-format string has {wanted} placeholder(s) but {got} substitution(s)")
    }
}

/// ## What it does
/// Checks for `printf`-style format strings that use the `*` specifier with
/// non-tuple values.
///
/// ## Why is this bad?
/// The use of the `*` specifier with non-tuple values will raise a
/// `TypeError` at runtime.
///
/// ## Example
/// ```python
/// from math import pi
///
/// "%(n).*f" % {"n": (2, pi)}
/// ```
///
/// Use instead:
/// ```python
/// from math import pi
///
/// "%.*f" % (2, pi)  # 3.14
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatStarRequiresSequence;

impl Violation for PercentFormatStarRequiresSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`%`-format string `*` specifier requires sequence".to_string()
    }
}

/// ## What it does
/// Checks for `printf`-style format strings with invalid format characters.
///
/// ## Why is this bad?
/// In `printf`-style format strings, the `%` character is used to indicate
/// placeholders. If a `%` character is not followed by a valid format
/// character, it will raise a `ValueError` at runtime.
///
/// ## Example
/// ```python
/// "Hello, %S" % "world"
/// ```
///
/// Use instead:
/// ```python
/// "Hello, %s" % "world"
/// ```
///
/// ## References
/// - [Python documentation: `printf`-style String Formatting](https://docs.python.org/3/library/stdtypes.html#printf-style-string-formatting)
#[derive(ViolationMetadata)]
pub(crate) struct PercentFormatUnsupportedFormatCharacter {
    pub(crate) char: char,
}

impl Violation for PercentFormatUnsupportedFormatCharacter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatUnsupportedFormatCharacter { char } = self;
        format!("`%`-format string has unsupported format character `{char}`")
    }
}

/// ## What it does
/// Checks for `str.format` calls with invalid format strings.
///
/// ## Why is this bad?
/// Invalid format strings will raise a `ValueError`.
///
/// ## Example
/// ```python
/// "{".format(foo)
/// ```
///
/// Use instead:
/// ```python
/// "{}".format(foo)
/// ```
///
/// ## References
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[derive(ViolationMetadata)]
pub(crate) struct StringDotFormatInvalidFormat {
    pub(crate) message: String,
}

impl Violation for StringDotFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatInvalidFormat { message } = self;
        format!("`.format` call has invalid format string: {message}")
    }
}

/// ## What it does
/// Checks for `str.format` calls with unused keyword arguments.
///
/// ## Why is this bad?
/// Unused keyword arguments are redundant, and often indicative of a mistake.
/// They should be removed.
///
/// ## Example
/// ```python
/// "Hello, {name}".format(greeting="Hello", name="World")
/// ```
///
/// Use instead:
/// ```python
/// "Hello, {name}".format(name="World")
/// ```
///
/// ## References
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[derive(ViolationMetadata)]
pub(crate) struct StringDotFormatExtraNamedArguments {
    missing: Vec<Name>,
}

impl Violation for StringDotFormatExtraNamedArguments {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call has unused named argument(s): {message}")
    }

    fn fix_title(&self) -> Option<String> {
        let StringDotFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        Some(format!("Remove extra named arguments: {message}"))
    }
}

/// ## What it does
/// Checks for `str.format` calls with unused positional arguments.
///
/// ## Why is this bad?
/// Unused positional arguments are redundant, and often indicative of a mistake.
/// They should be removed.
///
/// ## Example
/// ```python
/// "Hello, {0}".format("world", "!")
/// ```
///
/// Use instead:
/// ```python
/// "Hello, {0}".format("world")
/// ```
///
/// ## References
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[derive(ViolationMetadata)]
pub(crate) struct StringDotFormatExtraPositionalArguments {
    missing: Vec<String>,
}

impl Violation for StringDotFormatExtraPositionalArguments {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraPositionalArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call has unused arguments at position(s): {message}")
    }

    fn fix_title(&self) -> Option<String> {
        let StringDotFormatExtraPositionalArguments { missing } = self;
        let message = missing.join(", ");
        Some(format!(
            "Remove extra positional arguments at position(s): {message}"
        ))
    }
}

/// ## What it does
/// Checks for `str.format` calls with placeholders that are missing arguments.
///
/// ## Why is this bad?
/// In `str.format` calls, omitting arguments for placeholders will raise a
/// `KeyError` at runtime.
///
/// ## Example
/// ```python
/// "{greeting}, {name}".format(name="World")
/// ```
///
/// Use instead:
/// ```python
/// "{greeting}, {name}".format(greeting="Hello", name="World")
/// ```
///
/// ## References
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[derive(ViolationMetadata)]
pub(crate) struct StringDotFormatMissingArguments {
    missing: Vec<String>,
}

impl Violation for StringDotFormatMissingArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatMissingArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call is missing argument(s) for placeholder(s): {message}")
    }
}

/// ## What it does
/// Checks for `str.format` calls that mix automatic and manual numbering.
///
/// ## Why is this bad?
/// In `str.format` calls, mixing automatic and manual numbering will raise a
/// `ValueError` at runtime.
///
/// ## Example
/// ```python
/// "{0}, {}".format("Hello", "World")
/// ```
///
/// Use instead:
/// ```python
/// "{0}, {1}".format("Hello", "World")
/// ```
///
/// Or:
/// ```python
/// "{}, {}".format("Hello", "World")
/// ```
///
/// ## References
/// - [Python documentation: `str.format`](https://docs.python.org/3/library/stdtypes.html#str.format)
#[derive(ViolationMetadata)]
pub(crate) struct StringDotFormatMixingAutomatic;

impl Violation for StringDotFormatMixingAutomatic {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`.format` string mixes automatic and manual numbering".to_string()
    }
}

fn has_star_star_kwargs(keywords: &[Keyword]) -> bool {
    keywords
        .iter()
        .any(|keyword| matches!(keyword, Keyword { arg: None, .. }))
}

fn has_star_args(args: &[Expr]) -> bool {
    args.iter().any(Expr::is_starred_expr)
}

/// F502
pub(crate) fn percent_format_expected_mapping(
    checker: &Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: TextRange,
) {
    if !summary.keywords.is_empty() {
        // Tuple, List, Set (+comprehensions)
        match right {
            Expr::List(_)
            | Expr::Tuple(_)
            | Expr::Set(_)
            | Expr::ListComp(_)
            | Expr::SetComp(_)
            | Expr::Generator(_) => {
                checker.report_diagnostic(Diagnostic::new(PercentFormatExpectedMapping, location));
            }
            _ => {}
        }
    }
}

/// F503
pub(crate) fn percent_format_expected_sequence(
    checker: &Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: TextRange,
) {
    if summary.num_positional > 1 && matches!(right, Expr::Dict(_) | Expr::DictComp(_)) {
        checker.report_diagnostic(Diagnostic::new(PercentFormatExpectedSequence, location));
    }
}

/// F504
pub(crate) fn percent_format_extra_named_arguments(
    checker: &Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: TextRange,
) {
    if summary.num_positional > 0 {
        return;
    }
    let Expr::Dict(dict) = &right else {
        return;
    };
    // If any of the keys are spread, abort.
    if dict.iter_keys().any(|key| key.is_none()) {
        return;
    }

    let missing: Vec<(usize, &str)> = dict
        .iter_keys()
        .enumerate()
        .filter_map(|(index, key)| match key {
            Some(Expr::StringLiteral(ast::ExprStringLiteral { value, .. })) => {
                if summary.keywords.contains(value.to_str()) {
                    None
                } else {
                    Some((index, value.to_str()))
                }
            }
            _ => None,
        })
        .collect();

    if missing.is_empty() {
        return;
    }

    let names: Vec<String> = missing
        .iter()
        .map(|(_, name)| (*name).to_string())
        .collect();
    let mut diagnostic = Diagnostic::new(
        PercentFormatExtraNamedArguments { missing: names },
        location,
    );
    let indexes: Vec<usize> = missing.iter().map(|(index, _)| *index).collect();
    diagnostic.try_set_fix(|| {
        let edit = remove_unused_format_arguments_from_dict(
            &indexes,
            dict,
            checker.locator(),
            checker.stylist(),
        )?;
        Ok(Fix::safe_edit(edit))
    });
    checker.report_diagnostic(diagnostic);
}

/// F505
pub(crate) fn percent_format_missing_arguments(
    checker: &Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: TextRange,
) {
    if summary.num_positional > 0 {
        return;
    }

    let Expr::Dict(dict) = &right else {
        return;
    };

    if dict.iter_keys().any(|key| key.is_none()) {
        return; // contains **x splat
    }

    let mut keywords = FxHashSet::default();
    for key in dict.iter_keys().flatten() {
        match key {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                keywords.insert(value.to_str());
            }
            _ => {
                return; // Dynamic keys present
            }
        }
    }

    let missing: Vec<&String> = summary
        .keywords
        .iter()
        .filter(|k| !keywords.contains(k.as_str()))
        .collect();

    if !missing.is_empty() {
        checker.report_diagnostic(Diagnostic::new(
            PercentFormatMissingArgument {
                missing: missing.iter().map(|&s| s.clone()).collect(),
            },
            location,
        ));
    }
}

/// F506
pub(crate) fn percent_format_mixed_positional_and_named(
    checker: &Checker,
    summary: &CFormatSummary,
    location: TextRange,
) {
    if !(summary.num_positional == 0 || summary.keywords.is_empty()) {
        checker.report_diagnostic(Diagnostic::new(
            PercentFormatMixedPositionalAndNamed,
            location,
        ));
    }
}

/// F507
pub(crate) fn percent_format_positional_count_mismatch(
    checker: &Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: TextRange,
) {
    if !summary.keywords.is_empty() {
        return;
    }

    if let Expr::Tuple(tuple) = right {
        let mut found = 0;
        for element in tuple {
            if element.is_starred_expr() {
                return;
            }
            found += 1;
        }

        if found != summary.num_positional {
            checker.report_diagnostic(Diagnostic::new(
                PercentFormatPositionalCountMismatch {
                    wanted: summary.num_positional,
                    got: found,
                },
                location,
            ));
        }
    }
}

/// F508
pub(crate) fn percent_format_star_requires_sequence(
    checker: &Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: TextRange,
) {
    if summary.starred {
        match right {
            Expr::Dict(_) | Expr::DictComp(_) => checker
                .report_diagnostic(Diagnostic::new(PercentFormatStarRequiresSequence, location)),
            _ => {}
        }
    }
}

/// F522
pub(crate) fn string_dot_format_extra_named_arguments(
    checker: &Checker,
    call: &ast::ExprCall,
    summary: &FormatSummary,
    keywords: &[Keyword],
) {
    // If there are any **kwargs, abort.
    if has_star_star_kwargs(keywords) {
        return;
    }

    let keywords = keywords
        .iter()
        .filter_map(|Keyword { arg, .. }| arg.as_ref());

    let missing: Vec<(usize, &Name)> = keywords
        .enumerate()
        .filter_map(|(index, keyword)| {
            if summary.keywords.contains(keyword.id()) {
                None
            } else {
                Some((index, &keyword.id))
            }
        })
        .collect();

    if missing.is_empty() {
        return;
    }

    let names: Vec<Name> = missing.iter().map(|(_, name)| (*name).clone()).collect();
    let mut diagnostic = Diagnostic::new(
        StringDotFormatExtraNamedArguments { missing: names },
        call.range(),
    );
    let indexes: Vec<usize> = missing.iter().map(|(index, _)| *index).collect();
    diagnostic.try_set_fix(|| {
        let edit = remove_unused_keyword_arguments_from_format_call(
            &indexes,
            call,
            checker.locator(),
            checker.stylist(),
        )?;
        Ok(Fix::safe_edit(edit))
    });
    checker.report_diagnostic(diagnostic);
}

/// F523
pub(crate) fn string_dot_format_extra_positional_arguments(
    checker: &Checker,
    call: &ast::ExprCall,
    summary: &FormatSummary,
    args: &[Expr],
) {
    // We can only fix if the positional arguments we're removing don't require re-indexing
    // the format string itself. For example, we can't fix `"{1}{2}".format(0, 1, 2)"`, since
    // this requires changing the format string to `"{0}{1}"`. But we can fix
    // `"{0}{1}".format(0, 1, 2)`, since this only requires modifying the call arguments.
    fn is_contiguous_from_end<T>(indexes: &[usize], target: &[T]) -> bool {
        if indexes.is_empty() {
            return true;
        }

        let mut expected_index = target.len() - 1;
        for &index in indexes.iter().rev() {
            if index != expected_index {
                return false;
            }

            if expected_index == 0 {
                break;
            }

            expected_index -= 1;
        }

        true
    }

    let missing: Vec<usize> = args
        .iter()
        .enumerate()
        .filter(|(i, arg)| {
            !(arg.is_starred_expr() || summary.autos.contains(i) || summary.indices.contains(i))
        })
        .map(|(i, _)| i)
        .collect();

    if missing.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        StringDotFormatExtraPositionalArguments {
            missing: missing
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<String>>(),
        },
        call.range(),
    );

    if is_contiguous_from_end(&missing, args) {
        diagnostic.try_set_fix(|| {
            let edit = remove_unused_positional_arguments_from_format_call(
                &missing,
                call,
                checker.locator(),
                checker.stylist(),
            )?;
            Ok(Fix::safe_edit(edit))
        });
    }

    checker.report_diagnostic(diagnostic);
}

/// F524
pub(crate) fn string_dot_format_missing_argument(
    checker: &Checker,
    call: &ast::ExprCall,
    summary: &FormatSummary,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if has_star_args(args) || has_star_star_kwargs(keywords) {
        return;
    }

    let keywords: FxHashSet<_> = keywords
        .iter()
        .filter_map(|k| {
            let Keyword { arg, .. } = &k;
            arg.as_ref().map(ruff_python_ast::Identifier::id)
        })
        .collect();

    let missing: Vec<String> = summary
        .autos
        .iter()
        .chain(&summary.indices)
        .filter(|&&i| i >= args.len())
        .map(ToString::to_string)
        .chain(
            summary
                .keywords
                .iter()
                .filter(|k| !keywords.contains(*k))
                .map(ToString::to_string),
        )
        .collect();

    if !missing.is_empty() {
        checker.report_diagnostic(Diagnostic::new(
            StringDotFormatMissingArguments { missing },
            call.range(),
        ));
    }
}

/// F525
pub(crate) fn string_dot_format_mixing_automatic(
    checker: &Checker,
    call: &ast::ExprCall,
    summary: &FormatSummary,
) {
    if !(summary.autos.is_empty() || summary.indices.is_empty()) {
        checker.report_diagnostic(Diagnostic::new(
            StringDotFormatMixingAutomatic,
            call.range(),
        ));
    }
}
