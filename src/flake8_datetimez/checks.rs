use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, KeywordData, Located};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

fn get_tzinfo_in_keywords(keywords: &[Keyword]) -> Option<&Keyword> {
    keywords.iter().find(|keyword| {
        let KeywordData { arg, .. } = &keyword.node;
        arg.as_ref().map_or(false, |_arg| _arg == "tzinfo")
    })
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

// a.b.c(..) -> ['a', 'b', 'c']
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
    let is_datetime_datetime_func = is_expected_func_call(func, &["datetime", "datetime"]);
    let is_datetime_func = is_expected_func_call(func, &["datetime"]);

    if !is_datetime_datetime_func && !is_datetime_func {
        return None;
    }

    // no args
    if is_datetime_datetime_func && args.len() < 8 && keywords.is_empty() {
        return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
    }

    // no args unqualified
    if is_datetime_func && args.len() < 8 && keywords.is_empty() {
        return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
    }

    // none args
    if is_datetime_datetime_func && args.len() == 8 && is_const_none(&args[7]) {
        return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
    }

    let tzinfo = get_tzinfo_in_keywords(keywords);

    // no kwargs
    if is_datetime_datetime_func && tzinfo.is_none() {
        return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
    }

    // none kwargs
    if is_datetime_datetime_func {
        if let Some(Located {
            node: KeywordData { value, .. },
            ..
        }) = tzinfo
        {
            if is_const_none(value) {
                return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
            }
        }
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
