use rustpython_ast::{Constant, Expr, ExprKind, Keyword, KeywordData};

use crate::ast::helpers::{collect_call_paths, dealias_call_path, match_call_path};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// Return the `Keyword` with the given name, if it's present in the list of
/// `Keyword` arguments.
fn get_keyword_in_keywords<'a>(keywords: &'a [Keyword], keyword_name: &str) -> Option<&'a Keyword> {
    keywords.iter().find(|keyword| {
        let KeywordData { arg, .. } = &keyword.node;
        arg.as_ref().map_or(false, |arg| arg == keyword_name)
    })
}

/// Return `true` if an `Expr` is `None`.
fn is_const_none(expr: &Expr) -> bool {
    matches!(
        &expr.node,
        ExprKind::Constant {
            value: Constant::None,
            kind: None
        },
    )
}

/// Return `true` if a keyword argument is present with a non-`None` value.
fn has_not_none_keyword_in_keywords(keywords: &[Keyword], keyword: &str) -> bool {
    if let Some(keyword_data) = get_keyword_in_keywords(keywords, keyword) {
        if !is_const_none(&keyword_data.node.value) {
            return true;
        }
    }
    false
}

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

    // no args / no args unqualified
    if args.len() < 8 && keywords.is_empty() {
        checker.add_check(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
        return;
    }

    // none args
    if args.len() == 8 && is_const_none(&args[7]) {
        checker.add_check(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
        return;
    }

    // no kwargs / none kwargs
    if !has_not_none_keyword_in_keywords(keywords, "tzinfo") {
        checker.add_check(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
        
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
        checker.add_check(Check::new(CheckKind::CallDatetimeToday, location));
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
        checker.add_check(Check::new(CheckKind::CallDatetimeUtcnow, location));
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
        checker.add_check(Check::new(
            CheckKind::CallDatetimeUtcfromtimestamp,
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
        checker.add_check(Check::new(
            CheckKind::CallDatetimeNowWithoutTzinfo,
            location,
        ));
        return;
    }

    // none args
    if !args.is_empty() && is_const_none(&args[0]) {
        checker.add_check(Check::new(
            CheckKind::CallDatetimeNowWithoutTzinfo,
            location,
        ));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_not_none_keyword_in_keywords(keywords, "tz") {
        checker.add_check(Check::new(
            CheckKind::CallDatetimeNowWithoutTzinfo,
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
        checker.add_check(Check::new(CheckKind::CallDatetimeFromtimestamp, location));
        return;
    }

    // none args
    if args.len() > 1 && is_const_none(&args[1]) {
        checker.add_check(Check::new(CheckKind::CallDatetimeFromtimestamp, location));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_not_none_keyword_in_keywords(keywords, "tz") {
        checker.add_check(Check::new(CheckKind::CallDatetimeFromtimestamp, location));
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

    if let Some(ExprKind::Constant {
        value: Constant::Str(format),
        kind: None,
    }) = args.get(1).as_ref().map(|arg| &arg.node)
    {
        if !format.contains("%z") {
            checker.add_check(Check::new(
                CheckKind::CallDatetimeStrptimeWithoutZone,
                location,
            ));
        }
    }
}

/// DTZ011
pub fn call_date_today(checker: &mut Checker, func: &Expr, location: Range) {
    let call_path = dealias_call_path(collect_call_paths(func), &checker.import_aliases);
    if match_call_path(&call_path, "datetime.date", "today", &checker.from_imports) {
        checker.add_check(Check::new(CheckKind::CallDateToday, location));
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
        checker.add_check(Check::new(CheckKind::CallDateFromtimestamp, location));
    }
}
