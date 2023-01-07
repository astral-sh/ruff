use rustpython_ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::helpers::{
    collect_call_paths, dealias_call_path, has_non_none_keyword, is_const_none, match_call_path,
};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violations;

pub fn call_datetime_without_tzinfo(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if !match_call_path(&call_path, "datetime", "datetime", &checker.from_imports) {
        return;
    }

    // No positional arg: keyword is missing or constant None.
    if args.len() < 8 && !has_non_none_keyword(keywords, "tzinfo") {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeWithoutTzinfo,
            location,
        ));
        return;
    }

    // Positional arg: is constant None.
    if args.len() >= 8 && is_const_none(&args[7]) {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeWithoutTzinfo,
            location,
        ));
    }
}

/// DTZ002
pub fn call_datetime_today(checker: &mut Checker, func: &Expr, location: Range) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if match_call_path(
        &call_path,
        "datetime.datetime",
        "today",
        &checker.from_imports,
    ) {
        checker
            .checks
            .push(Diagnostic::new(violations::CallDatetimeToday, location));
    }
}

/// DTZ003
pub fn call_datetime_utcnow(checker: &mut Checker, func: &Expr, location: Range) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if match_call_path(
        &call_path,
        "datetime.datetime",
        "utcnow",
        &checker.from_imports,
    ) {
        checker
            .checks
            .push(Diagnostic::new(violations::CallDatetimeUtcnow, location));
    }
}

/// DTZ004
pub fn call_datetime_utcfromtimestamp(checker: &mut Checker, func: &Expr, location: Range) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if match_call_path(
        &call_path,
        "datetime.datetime",
        "utcfromtimestamp",
        &checker.from_imports,
    ) {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeUtcfromtimestamp,
            location,
        ));
    }
}

/// DTZ005
pub fn call_datetime_now_without_tzinfo(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if !match_call_path(
        &call_path,
        "datetime.datetime",
        "now",
        &checker.from_imports,
    ) {
        return;
    }

    // no args / no args unqualified
    if args.is_empty() && keywords.is_empty() {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeNowWithoutTzinfo,
            location,
        ));
        return;
    }

    // none args
    if !args.is_empty() && is_const_none(&args[0]) {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeNowWithoutTzinfo,
            location,
        ));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_non_none_keyword(keywords, "tz") {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeNowWithoutTzinfo,
            location,
        ));
    }
}

/// DTZ006
pub fn call_datetime_fromtimestamp(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if !match_call_path(
        &call_path,
        "datetime.datetime",
        "fromtimestamp",
        &checker.from_imports,
    ) {
        return;
    }

    // no args / no args unqualified
    if args.len() < 2 && keywords.is_empty() {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeFromtimestamp,
            location,
        ));
        return;
    }

    // none args
    if args.len() > 1 && is_const_none(&args[1]) {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeFromtimestamp,
            location,
        ));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_non_none_keyword(keywords, "tz") {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeFromtimestamp,
            location,
        ));
    }
}

/// DTZ007
pub fn call_datetime_strptime_without_zone(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    location: Range,
) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if !match_call_path(
        &call_path,
        "datetime.datetime",
        "strptime",
        &checker.from_imports,
    ) {
        return;
    }

    // Does the `strptime` call contain a format string with a timezone specifier?
    if let Some(ExprKind::Constant {
        value: Constant::Str(format),
        kind: None,
    }) = args.get(1).as_ref().map(|arg| &arg.node)
    {
        if format.contains("%z") {
            return;
        }
    };

    let (Some(grandparent), Some(parent)) = (checker.current_expr_grandparent(), checker.current_expr_parent()) else {
        checker.checks.push(Diagnostic::new(
            violations::CallDatetimeStrptimeWithoutZone,
            location,
        ));
        return;
    };

    if let ExprKind::Call { keywords, .. } = &grandparent.node {
        if let ExprKind::Attribute { attr, .. } = &parent.node {
            // Ex) `datetime.strptime(...).astimezone()`
            if attr == "astimezone" {
                return;
            }

            // Ex) `datetime.strptime(...).replace(tzinfo=UTC)`
            if attr == "replace" {
                if has_non_none_keyword(keywords, "tzinfo") {
                    return;
                }
            }
        }
    }

    checker.checks.push(Diagnostic::new(
        violations::CallDatetimeStrptimeWithoutZone,
        location,
    ));
}

/// DTZ011
pub fn call_date_today(checker: &mut Checker, func: &Expr, location: Range) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if match_call_path(&call_path, "datetime.date", "today", &checker.from_imports) {
        checker
            .checks
            .push(Diagnostic::new(violations::CallDateToday, location));
    }
}

/// DTZ012
pub fn call_date_fromtimestamp(checker: &mut Checker, func: &Expr, location: Range) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if match_call_path(
        &call_path,
        "datetime.date",
        "fromtimestamp",
        &checker.from_imports,
    ) {
        checker
            .checks
            .push(Diagnostic::new(violations::CallDateFromtimestamp, location));
    }
}
