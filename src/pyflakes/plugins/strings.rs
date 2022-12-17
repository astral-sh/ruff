use std::string::ToString;

use rustc_hash::FxHashSet;
use rustpython_ast::{Keyword, KeywordData};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::pyflakes::cformat::CFormatSummary;
use crate::pyflakes::fixes::{
    remove_unused_format_arguments_from_dict, remove_unused_keyword_arguments_from_format_call,
};
use crate::pyflakes::format::FormatSummary;

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
            | ExprKind::GeneratorExp { .. } => checker.add_check(Check::new(
                CheckKind::PercentFormatExpectedMapping,
                location,
            )),
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
        checker.add_check(Check::new(
            CheckKind::PercentFormatExpectedSequence,
            location,
        ));
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
    let ExprKind::Dict { keys, values } = &right.node else {
        return;
    };
    if values.len() > keys.len() {
        return; // contains **x splat
    }

    let missing: Vec<&str> = keys
        .iter()
        .filter_map(|k| match &k.node {
            // We can only check that string literals exist
            ExprKind::Constant {
                value: Constant::Str(value),
                ..
            } => {
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

    let mut check = Check::new(
        CheckKind::PercentFormatExtraNamedArguments(
            missing.iter().map(|&arg| arg.to_string()).collect(),
        ),
        location,
    );
    if checker.patch(check.kind.code()) {
        if let Ok(fix) = remove_unused_format_arguments_from_dict(&missing, right, checker.locator)
        {
            check.amend(fix);
        }
    }
    checker.add_check(check);
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

    if let ExprKind::Dict { keys, values } = &right.node {
        if values.len() > keys.len() {
            return; // contains **x splat
        }

        let mut keywords = FxHashSet::default();
        for key in keys {
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
            checker.add_check(Check::new(
                CheckKind::PercentFormatMissingArgument(
                    missing.iter().map(|&s| s.clone()).collect(),
                ),
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
        checker.add_check(Check::new(
            CheckKind::PercentFormatMixedPositionalAndNamed,
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
                checker.add_check(Check::new(
                    CheckKind::PercentFormatPositionalCountMismatch(summary.num_positional, found),
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
            ExprKind::Dict { .. } | ExprKind::DictComp { .. } => checker.add_check(Check::new(
                CheckKind::PercentFormatStarRequiresSequence,
                location,
            )),
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

    let mut check = Check::new(
        CheckKind::StringDotFormatExtraNamedArguments(
            missing.iter().map(|&arg| arg.to_string()).collect(),
        ),
        location,
    );
    if checker.patch(check.kind.code()) {
        if let Ok(fix) =
            remove_unused_keyword_arguments_from_format_call(&missing, location, checker.locator)
        {
            check.amend(fix);
        }
    }
    checker.add_check(check);
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

    checker.add_check(Check::new(
        CheckKind::StringDotFormatExtraPositionalArguments(
            missing
                .iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<String>>(),
        ),
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
        checker.add_check(Check::new(
            CheckKind::StringDotFormatMissingArguments(missing),
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
        checker.add_check(Check::new(
            CheckKind::StringDotFormatMixingAutomatic,
            location,
        ));
    }
}
