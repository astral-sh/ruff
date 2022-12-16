use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, KeywordData, Located};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

fn get_keyword_in_keywords<'a>(keywords: &'a [Keyword], keyword_name: &str) -> Option<&'a Keyword> {
    keywords.iter().find(|keyword| {
        let KeywordData { arg, .. } = &keyword.node;
        arg.as_ref().map_or(false, |_arg| _arg == keyword_name)
    })
}

// aaa(<keyword>=<not_none>)
fn has_not_none_keyword_in_keywords(keywords: &[Keyword], keyword: &str) -> bool {
    if let Some(keyword_data) = get_keyword_in_keywords(keywords, keyword) {
        if !is_const_none(&keyword_data.node.value) {
            return true;
        }
    }
    false
}

fn is_const_none(value: &Located<ExprKind>) -> bool {
    matches!(
        &value.node,
        ExprKind::Constant {
            value: Constant::None,
            kind: None
        },
    )
}

// aaa.bbb.ccc(..) -> ['aaa', 'bbb', 'ccc']
fn get_call_parts(func: &Expr) -> Vec<&String> {
    match &func.node {
        ExprKind::Attribute {
            value,
            attr,
            ctx: ExprContext::Load,
        } => {
            let mut parts = get_call_parts(value);
            parts.push(attr);
            parts
        }
        ExprKind::Name {
            id,
            ctx: ExprContext::Load,
        } => {
            vec![id]
        }
        _ => Vec::new(),
    }
}

fn is_expected_func_call(func: &Expr, func_call_parts: &[&str]) -> bool {
    if func_call_parts.is_empty() {
        return false;
    }
    let got_call_parts = get_call_parts(func);
    got_call_parts.len() == func_call_parts.len()
        && func_call_parts
            .iter()
            .zip(got_call_parts)
            .all(|(got, expected)| got == expected)
}

pub fn call_datetime_without_tzinfo(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) -> Option<Check> {
    let check = Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));

    let is_datetime_datetime_func = is_expected_func_call(func, &["datetime", "datetime"]);
    let is_datetime_func = is_expected_func_call(func, &["datetime"]);

    if !is_datetime_datetime_func && !is_datetime_func {
        return None;
    }

    // no args / no args unqualified
    if args.len() < 8 && keywords.is_empty() {
        return check;
    }

    // none args
    if args.len() == 8 && is_const_none(&args[7]) {
        return check;
    }

    // no kwargs / none kwargs
    if !has_not_none_keyword_in_keywords(keywords, "tzinfo") {
        return check;
    }

    None
}

pub fn call_datetime_today(func: &Expr, location: Range) -> Option<Check> {
    let is_datetime_today_func = is_expected_func_call(func, &["datetime", "today"]);
    let is_datetime_datetime_today_func =
        is_expected_func_call(func, &["datetime", "datetime", "today"]);
    if is_datetime_today_func || is_datetime_datetime_today_func {
        return Some(Check::new(CheckKind::CallDatetimeToday, location));
    }
    None
}

pub fn call_datetime_utcnow(func: &Expr, location: Range) -> Option<Check> {
    let is_datetime_utcnow_func = is_expected_func_call(func, &["datetime", "utcnow"]);
    let is_datetime_datetime_utcnow_func =
        is_expected_func_call(func, &["datetime", "datetime", "utcnow"]);
    if is_datetime_utcnow_func || is_datetime_datetime_utcnow_func {
        return Some(Check::new(CheckKind::CallDatetimeUtcnow, location));
    }
    None
}

pub fn call_datetime_utcfromtimestamp(func: &Expr, location: Range) -> Option<Check> {
    let is_datetime_utcfromtimestamp_func =
        is_expected_func_call(func, &["datetime", "utcfromtimestamp"]);
    let is_datetime_datetime_utcfromtimestamp_func =
        is_expected_func_call(func, &["datetime", "datetime", "utcfromtimestamp"]);
    if is_datetime_utcfromtimestamp_func || is_datetime_datetime_utcfromtimestamp_func {
        return Some(Check::new(
            CheckKind::CallDatetimeUtcfromtimestamp,
            location,
        ));
    }
    None
}

pub fn call_datetime_now_without_tzinfo(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) -> Option<Check> {
    let check = Some(Check::new(
        CheckKind::CallDatetimeNowWithoutTzinfo,
        location,
    ));

    let is_datetime_now_func = is_expected_func_call(func, &["datetime", "now"]);
    let is_datetime_datetime_now_func =
        is_expected_func_call(func, &["datetime", "datetime", "now"]);
    if !is_datetime_now_func && !is_datetime_datetime_now_func {
        return None;
    }

    // no args / no args unqualified
    if args.is_empty() && keywords.is_empty() {
        return check;
    }

    // none args
    if !args.is_empty() && is_const_none(&args[0]) {
        return check;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() {
        if has_not_none_keyword_in_keywords(keywords, "tz") {
            return None;
        }
        return check;
    }

    None
}

pub fn call_datetime_fromtimestamp(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) -> Option<Check> {
    let check = Some(Check::new(CheckKind::CallDatetimeFromtimestamp, location));

    let is_datetime_fromtimestamp_func =
        is_expected_func_call(func, &["datetime", "fromtimestamp"]);
    let is_datetime_datetime_fromtimestamp_func =
        is_expected_func_call(func, &["datetime", "datetime", "fromtimestamp"]);
    if !is_datetime_fromtimestamp_func && !is_datetime_datetime_fromtimestamp_func {
        return None;
    }

    // no args / no args unqualified
    if args.len() < 2 && keywords.is_empty() {
        return check;
    }

    // none args
    if args.len() > 1 && is_const_none(&args[1]) {
        return check;
    }

    // wrong keywords / none keyword
    if !keywords.is_empty() {
        if has_not_none_keyword_in_keywords(keywords, "tz") {
            return None;
        }
        return check;
    }

    None
}
