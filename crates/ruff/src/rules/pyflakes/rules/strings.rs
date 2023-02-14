use std::string::ToString;

use log::error;
use ruff_macros::{define_violation, derive_message_formats};
use rustc_hash::FxHashSet;
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword, KeywordData};

use super::super::cformat::CFormatSummary;
use super::super::fixes::{
    remove_unused_format_arguments_from_dict, remove_unused_keyword_arguments_from_format_call,
};
use super::super::format::FormatSummary;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::{AlwaysAutofixableViolation, Violation};

define_violation!(
    pub struct PercentFormatInvalidFormat {
        pub message: String,
    }
);
impl Violation for PercentFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatInvalidFormat { message } = self;
        format!("`%`-format string has invalid format string: {message}")
    }
}

define_violation!(
    pub struct PercentFormatExpectedMapping;
);
impl Violation for PercentFormatExpectedMapping {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string expected mapping but got sequence")
    }
}

define_violation!(
    pub struct PercentFormatExpectedSequence;
);
impl Violation for PercentFormatExpectedSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string expected sequence but got mapping")
    }
}

define_violation!(
    pub struct PercentFormatExtraNamedArguments {
        pub missing: Vec<String>,
    }
);
impl AlwaysAutofixableViolation for PercentFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("`%`-format string has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let PercentFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

define_violation!(
    pub struct PercentFormatMissingArgument {
        pub missing: Vec<String>,
    }
);
impl Violation for PercentFormatMissingArgument {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatMissingArgument { missing } = self;
        let message = missing.join(", ");
        format!("`%`-format string is missing argument(s) for placeholder(s): {message}")
    }
}

define_violation!(
    pub struct PercentFormatMixedPositionalAndNamed;
);
impl Violation for PercentFormatMixedPositionalAndNamed {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string has mixed positional and named placeholders")
    }
}

define_violation!(
    pub struct PercentFormatPositionalCountMismatch {
        pub wanted: usize,
        pub got: usize,
    }
);
impl Violation for PercentFormatPositionalCountMismatch {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatPositionalCountMismatch { wanted, got } = self;
        format!("`%`-format string has {wanted} placeholder(s) but {got} substitution(s)")
    }
}

define_violation!(
    pub struct PercentFormatStarRequiresSequence;
);
impl Violation for PercentFormatStarRequiresSequence {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`%`-format string `*` specifier requires sequence")
    }
}

define_violation!(
    pub struct PercentFormatUnsupportedFormatCharacter {
        pub char: char,
    }
);
impl Violation for PercentFormatUnsupportedFormatCharacter {
    #[derive_message_formats]
    fn message(&self) -> String {
        let PercentFormatUnsupportedFormatCharacter { char } = self;
        format!("`%`-format string has unsupported format character '{char}'")
    }
}

define_violation!(
    pub struct StringDotFormatInvalidFormat {
        pub message: String,
    }
);
impl Violation for StringDotFormatInvalidFormat {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatInvalidFormat { message } = self;
        format!("`.format` call has invalid format string: {message}")
    }
}

define_violation!(
    pub struct StringDotFormatExtraNamedArguments {
        pub missing: Vec<String>,
    }
);
impl AlwaysAutofixableViolation for StringDotFormatExtraNamedArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call has unused named argument(s): {message}")
    }

    fn autofix_title(&self) -> String {
        let StringDotFormatExtraNamedArguments { missing } = self;
        let message = missing.join(", ");
        format!("Remove extra named arguments: {message}")
    }
}

define_violation!(
    pub struct StringDotFormatExtraPositionalArguments {
        pub missing: Vec<String>,
    }
);
impl Violation for StringDotFormatExtraPositionalArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatExtraPositionalArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call has unused arguments at position(s): {message}")
    }
}

define_violation!(
    pub struct StringDotFormatMissingArguments {
        pub missing: Vec<String>,
    }
);
impl Violation for StringDotFormatMissingArguments {
    #[derive_message_formats]
    fn message(&self) -> String {
        let StringDotFormatMissingArguments { missing } = self;
        let message = missing.join(", ");
        format!("`.format` call is missing argument(s) for placeholder(s): {message}")
    }
}

define_violation!(
    pub struct StringDotFormatMixingAutomatic;
);
impl Violation for StringDotFormatMixingAutomatic {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.format` string mixes automatic and manual numbering")
    }
}

fn has_star_star_kwargs(keywords: &[Keyword]) -> bool {
    keywords.iter().any(|k| {
        let KeywordData { arg, .. } = &k.node;
        arg.is_none()
    })
}

fn has_star_args(args: &[Expr]) -> bool {
    args.iter()
        .any(|arg| matches!(&arg.node, ExprKind::Starred { .. }))
}

/// F502
pub(crate) fn percent_format_expected_mapping(
    checker: &mut Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: Range,
) {
    if !summary.keywords.is_empty() {
        // Tuple, List, Set (+comprehensions)
        match right.node {
            ExprKind::List { .. }
            | ExprKind::Tuple { .. }
            | ExprKind::Set { .. }
            | ExprKind::ListComp { .. }
            | ExprKind::SetComp { .. }
            | ExprKind::GeneratorExp { .. } => checker
                .diagnostics
                .push(Diagnostic::new(PercentFormatExpectedMapping, location)),
            _ => {}
        }
    }
}

/// F503
pub(crate) fn percent_format_expected_sequence(
    checker: &mut Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: Range,
) {
    if summary.num_positional > 1
        && matches!(
            right.node,
            ExprKind::Dict { .. } | ExprKind::DictComp { .. }
        )
    {
        checker
            .diagnostics
            .push(Diagnostic::new(PercentFormatExpectedSequence, location));
    }
}

/// F504
pub(crate) fn percent_format_extra_named_arguments(
    checker: &mut Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: Range,
) {
    if summary.num_positional > 0 {
        return;
    }
    let ExprKind::Dict { keys, .. } = &right.node else {
        return;
    };
    if keys.iter().any(std::option::Option::is_none) {
        return; // contains **x splat
    }

    let missing: Vec<&str> = keys
        .iter()
        .filter_map(|k| match k {
            Some(Expr {
                node:
                    ExprKind::Constant {
                        value: Constant::Str(value),
                        ..
                    },
                ..
            }) => {
                if summary.keywords.contains(value) {
                    None
                } else {
                    Some(value.as_str())
                }
            }
            _ => None,
        })
        .collect();

    if missing.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        PercentFormatExtraNamedArguments {
            missing: missing.iter().map(|&arg| arg.to_string()).collect(),
        },
        location,
    );
    if checker.patch(diagnostic.kind.rule()) {
        match remove_unused_format_arguments_from_dict(
            &missing,
            right,
            checker.locator,
            checker.stylist,
        ) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to remove unused format arguments: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// F505
pub(crate) fn percent_format_missing_arguments(
    checker: &mut Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: Range,
) {
    if summary.num_positional > 0 {
        return;
    }

    if let ExprKind::Dict { keys, .. } = &right.node {
        if keys.iter().any(std::option::Option::is_none) {
            return; // contains **x splat
        }

        let mut keywords = FxHashSet::default();
        for key in keys.iter().flatten() {
            match &key.node {
                ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } => {
                    keywords.insert(value);
                }
                _ => {
                    return; // Dynamic keys present
                }
            }
        }

        let missing: Vec<&String> = summary
            .keywords
            .iter()
            .filter(|k| !keywords.contains(k))
            .collect();

        if !missing.is_empty() {
            checker.diagnostics.push(Diagnostic::new(
                PercentFormatMissingArgument {
                    missing: missing.iter().map(|&s| s.clone()).collect(),
                },
                location,
            ));
        }
    }
}

/// F506
pub(crate) fn percent_format_mixed_positional_and_named(
    checker: &mut Checker,
    summary: &CFormatSummary,
    location: Range,
) {
    if !(summary.num_positional == 0 || summary.keywords.is_empty()) {
        checker.diagnostics.push(Diagnostic::new(
            PercentFormatMixedPositionalAndNamed,
            location,
        ));
    }
}

/// F507
pub(crate) fn percent_format_positional_count_mismatch(
    checker: &mut Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: Range,
) {
    if !summary.keywords.is_empty() {
        return;
    }

    match &right.node {
        ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } | ExprKind::Set { elts, .. } => {
            let mut found = 0;
            for elt in elts {
                if let ExprKind::Starred { .. } = &elt.node {
                    return;
                }
                found += 1;
            }

            if found != summary.num_positional {
                checker.diagnostics.push(Diagnostic::new(
                    PercentFormatPositionalCountMismatch {
                        wanted: summary.num_positional,
                        got: found,
                    },
                    location,
                ));
            }
        }
        _ => {}
    }
}

/// F508
pub(crate) fn percent_format_star_requires_sequence(
    checker: &mut Checker,
    summary: &CFormatSummary,
    right: &Expr,
    location: Range,
) {
    if summary.starred {
        match &right.node {
            ExprKind::Dict { .. } | ExprKind::DictComp { .. } => checker
                .diagnostics
                .push(Diagnostic::new(PercentFormatStarRequiresSequence, location)),
            _ => {}
        }
    }
}

/// F522
pub(crate) fn string_dot_format_extra_named_arguments(
    checker: &mut Checker,
    summary: &FormatSummary,
    keywords: &[Keyword],
    location: Range,
) {
    if has_star_star_kwargs(keywords) {
        return;
    }

    let keywords = keywords.iter().filter_map(|k| {
        let KeywordData { arg, .. } = &k.node;
        arg.as_ref()
    });

    let missing: Vec<&str> = keywords
        .filter_map(|arg| {
            if summary.keywords.contains(arg) {
                None
            } else {
                Some(arg.as_str())
            }
        })
        .collect();

    if missing.is_empty() {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        StringDotFormatExtraNamedArguments {
            missing: missing.iter().map(|&arg| arg.to_string()).collect(),
        },
        location,
    );
    if checker.patch(diagnostic.kind.rule()) {
        match remove_unused_keyword_arguments_from_format_call(
            &missing,
            location,
            checker.locator,
            checker.stylist,
        ) {
            Ok(fix) => {
                diagnostic.amend(fix);
            }
            Err(e) => error!("Failed to remove unused keyword arguments: {e}"),
        }
    }
    checker.diagnostics.push(diagnostic);
}

/// F523
pub(crate) fn string_dot_format_extra_positional_arguments(
    checker: &mut Checker,
    summary: &FormatSummary,
    args: &[Expr],
    location: Range,
) {
    if has_star_args(args) {
        return;
    }

    let missing: Vec<usize> = (0..args.len())
        .filter(|i| !(summary.autos.contains(i) || summary.indexes.contains(i)))
        .collect();

    if missing.is_empty() {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        StringDotFormatExtraPositionalArguments {
            missing: missing
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<String>>(),
        },
        location,
    ));
}

/// F524
pub(crate) fn string_dot_format_missing_argument(
    checker: &mut Checker,
    summary: &FormatSummary,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) {
    if has_star_args(args) || has_star_star_kwargs(keywords) {
        return;
    }

    let keywords: FxHashSet<_> = keywords
        .iter()
        .filter_map(|k| {
            let KeywordData { arg, .. } = &k.node;
            arg.as_ref()
        })
        .collect();

    let missing: Vec<String> = summary
        .autos
        .iter()
        .chain(summary.indexes.iter())
        .filter(|&&i| i >= args.len())
        .map(ToString::to_string)
        .chain(
            summary
                .keywords
                .iter()
                .filter(|k| !keywords.contains(k))
                .cloned(),
        )
        .collect();

    if !missing.is_empty() {
        checker.diagnostics.push(Diagnostic::new(
            StringDotFormatMissingArguments { missing },
            location,
        ));
    }
}

/// F525
pub(crate) fn string_dot_format_mixing_automatic(
    checker: &mut Checker,
    summary: &FormatSummary,
    location: Range,
) {
    if !(summary.autos.is_empty() || summary.indexes.is_empty()) {
        checker
            .diagnostics
            .push(Diagnostic::new(StringDotFormatMixingAutomatic, location));
    }
}
