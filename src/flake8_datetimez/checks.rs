use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Keyword, KeywordData, Located};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

fn get_tzinfo_in_keywords(keywords: &[Keyword]) -> Option<&Keyword> {
    keywords.iter().find(|keyword| {
        let KeywordData { arg, .. } = &keyword.node;
        arg.as_ref().map_or(false, |_arg| _arg == "tzinfo")
    })
}

fn is_none(value: &Located<ExprKind>) -> bool {
    matches!(
        &value.node,
        ExprKind::Constant {
            value: Constant::None,
            kind: None
        },
    )
}

fn is_datetime_func(func: &Expr) -> bool {
    match &func.node {
        ExprKind::Name {
            id,
            ctx: ExprContext::Load,
        } => id == "datetime",
        _ => false,
    }
}

fn is_datetime_datetime_func(func: &Expr) -> bool {
    match &func.node {
        ExprKind::Attribute {
            value,
            attr,
            ctx: ExprContext::Load,
        } => match &**value {
            Located {
                node:
                    ExprKind::Name {
                        id,
                        ctx: ExprContext::Load,
                    },
                ..
            } => id == "datetime" && attr == "datetime",
            _ => false,
        },
        _ => false,
    }
}

pub fn call_datetime_without_tzinfo(
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
    location: Range,
) -> Option<Check> {
    let is_datetime_datetime_func = is_datetime_datetime_func(func);
    let is_datetime_func = is_datetime_func(func);

    // no args
    if is_datetime_datetime_func && args.len() < 8 && keywords.is_empty() {
        return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
    }

    // no args unqualified
    if is_datetime_func && args.len() < 8 && keywords.is_empty() {
        return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
    }

    // none args
    if is_datetime_datetime_func && args.len() == 8 && is_none(&args[7]) {
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
            if is_none(value) {
                return Some(Check::new(CheckKind::CallDatetimeWithoutTzinfo, location));
            }
        }
    }

    None
}
