use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind, Keyword};

use crate::ast::helpers::{has_non_none_keyword, is_const_none};
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct CallDatetimeWithoutTzinfo;
);
impl Violation for CallDatetimeWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime()` without `tzinfo` argument is not allowed")
    }
}

define_violation!(
    pub struct CallDatetimeToday;
);
impl Violation for CallDatetimeToday {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.today()` is not allowed, use \
             `datetime.datetime.now(tz=)` instead"
        )
    }
}

define_violation!(
    pub struct CallDatetimeUtcnow;
);
impl Violation for CallDatetimeUtcnow {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.utcnow()` is not allowed, use \
             `datetime.datetime.now(tz=)` instead"
        )
    }
}

define_violation!(
    pub struct CallDatetimeUtcfromtimestamp;
);
impl Violation for CallDatetimeUtcfromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.utcfromtimestamp()` is not allowed, use \
             `datetime.datetime.fromtimestamp(ts, tz=)` instead"
        )
    }
}

define_violation!(
    pub struct CallDatetimeNowWithoutTzinfo;
);
impl Violation for CallDatetimeNowWithoutTzinfo {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("The use of `datetime.datetime.now()` without `tz` argument is not allowed")
    }
}

define_violation!(
    pub struct CallDatetimeFromtimestamp;
);
impl Violation for CallDatetimeFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.fromtimestamp()` without `tz` argument is not allowed"
        )
    }
}

define_violation!(
    pub struct CallDatetimeStrptimeWithoutZone;
);
impl Violation for CallDatetimeStrptimeWithoutZone {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.datetime.strptime()` without %z must be followed by \
             `.replace(tzinfo=)` or `.astimezone()`"
        )
    }
}

define_violation!(
    pub struct CallDateToday;
);
impl Violation for CallDateToday {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.date.today()` is not allowed, use \
             `datetime.datetime.now(tz=).date()` instead"
        )
    }
}

define_violation!(
    pub struct CallDateFromtimestamp;
);
impl Violation for CallDateFromtimestamp {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "The use of `datetime.date.fromtimestamp()` is not allowed, use \
             `datetime.datetime.fromtimestamp(ts, tz=).date()` instead"
        )
    }
}

pub fn call_datetime_without_tzinfo(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) {
    if !checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime"]
    }) {
        return;
    }

    // No positional arg: keyword is missing or constant None.
    if args.len() < 8 && !has_non_none_keyword(keywords, "tzinfo") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeWithoutTzinfo, location));
        return;
    }

    // Positional arg: is constant None.
    if args.len() >= 8 && is_const_none(&args[7]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeWithoutTzinfo, location));
    }
}

/// Checks for `datetime.datetime.today()`. (DTZ002)
///
/// ## Why is this bad?
///
/// It uses the system local timezone.
/// Use `datetime.datetime.now(tz=)` instead.
pub fn call_datetime_today(checker: &mut Checker, func: &Expr, location: Range) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime", "today"]
    }) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeToday, location));
    }
}

/// Checks for `datetime.datetime.today()`. (DTZ003)
///
/// ## Why is this bad?
///
/// Because naive `datetime` objects are treated by many `datetime` methods as
/// local times, it is preferred to use aware datetimes to represent times in
/// UTC. As such, the recommended way to create an object representing the
/// current time in UTC is by calling `datetime.now(timezone.utc)`.
pub fn call_datetime_utcnow(checker: &mut Checker, func: &Expr, location: Range) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime", "utcnow"]
    }) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeUtcnow, location));
    }
}

/// Checks for `datetime.datetime.utcfromtimestamp()`. (DTZ004)
///
/// ## Why is this bad?
///
/// Because naive `datetime` objects are treated by many `datetime` methods as
/// local times, it is preferred to use aware datetimes to represent times in
/// UTC. As such, the recommended way to create an object representing a
/// specific timestamp in UTC is by calling `datetime.fromtimestamp(timestamp,
/// tz=timezone.utc)`.
pub fn call_datetime_utcfromtimestamp(checker: &mut Checker, func: &Expr, location: Range) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime", "utcfromtimestamp"]
    }) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeUtcfromtimestamp, location));
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
    if !checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime", "now"]
    }) {
        return;
    }

    // no args / no args unqualified
    if args.is_empty() && keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, location));
        return;
    }

    // none args
    if !args.is_empty() && is_const_none(&args[0]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, location));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_non_none_keyword(keywords, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeNowWithoutTzinfo, location));
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
    if !checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime", "fromtimestamp"]
    }) {
        return;
    }

    // no args / no args unqualified
    if args.len() < 2 && keywords.is_empty() {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, location));
        return;
    }

    // none args
    if args.len() > 1 && is_const_none(&args[1]) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, location));
        return;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() && !has_non_none_keyword(keywords, "tz") {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDatetimeFromtimestamp, location));
    }
}

/// DTZ007
pub fn call_datetime_strptime_without_zone(
    checker: &mut Checker,
    func: &Expr,
    args: &[Expr],
    location: Range,
) {
    if !checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "datetime", "strptime"]
    }) {
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
        checker.diagnostics.push(Diagnostic::new(
            CallDatetimeStrptimeWithoutZone,
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

    checker
        .diagnostics
        .push(Diagnostic::new(CallDatetimeStrptimeWithoutZone, location));
}

/// Checks for `datetime.date.today()`. (DTZ011)
///
/// ## Why is this bad?
///
/// It uses the system local timezone.
/// Use `datetime.datetime.now(tz=).date()` instead.
pub fn call_date_today(checker: &mut Checker, func: &Expr, location: Range) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "date", "today"]
    }) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDateToday, location));
    }
}

/// Checks for `datetime.date.fromtimestamp()`. (DTZ012)
///
/// ## Why is this bad?
///
/// It uses the system local timezone.
/// Use `datetime.datetime.fromtimestamp(, tz=).date()` instead.
pub fn call_date_fromtimestamp(checker: &mut Checker, func: &Expr, location: Range) {
    if checker.resolve_call_path(func).map_or(false, |call_path| {
        call_path.as_slice() == ["datetime", "date", "fromtimestamp"]
    }) {
        checker
            .diagnostics
            .push(Diagnostic::new(CallDateFromtimestamp, location));
    }
}
