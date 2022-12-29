use rustc_hash::FxHashMap;
use rustpython_ast::{Expr, Located, Stmt};

use super::settings::BannedApi;
use crate::ast::helpers::match_call_path;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::flake8_tidy_imports::settings::Strictness;

pub fn banned_relative_import(
    stmt: &Stmt,
    level: Option<&usize>,
    strictness: &Strictness,
) -> Option<Check> {
    let strictness_level = match strictness {
        Strictness::All => 0,
        Strictness::Parents => 1,
    };
    if level? > &strictness_level {
        Some(Check::new(
            CheckKind::BannedRelativeImport(strictness.clone()),
            Range::from_located(stmt),
        ))
    } else {
        None
    }
}

pub fn name_is_banned<T>(
    located: &Located<T>,
    name: String,
    banned_apis: &FxHashMap<String, BannedApi>,
) -> Option<Check> {
    if let Some(ban) = banned_apis.get(&name) {
        return Some(Check::new(
            CheckKind::BannedApi {
                name,
                message: ban.msg.to_string(),
                attribute_access: false,
            },
            Range::from_located(located),
        ));
    }
    None
}

pub fn name_or_parent_is_banned<T>(
    located: &Located<T>,
    mut name: &str,
    banned_apis: &FxHashMap<String, BannedApi>,
) -> Option<Check> {
    loop {
        if let Some(ban) = banned_apis.get(name) {
            return Some(Check::new(
                CheckKind::BannedApi {
                    name: name.to_string(),
                    message: ban.msg.to_string(),
                    attribute_access: false,
                },
                Range::from_located(located),
            ));
        }

        match name.rfind('.') {
            Some(idx) => {
                name = &name[..idx];
            }
            None => break,
        }
    }
    None
}

pub fn banned_attribute_access(
    checker: &mut Checker,
    call_path: Vec<&str>,
    expr: &Expr,
    banned_apis: &FxHashMap<String, BannedApi>,
) -> Option<Check> {
    for (banned_path, ban) in banned_apis {
        if let Some((module, member)) = banned_path.rsplit_once('.') {
            if match_call_path(&call_path, module, member, &checker.from_imports) {
                return Some(Check::new(
                    CheckKind::BannedApi {
                        name: banned_path.to_string(),
                        message: ban.msg.to_string(),
                        attribute_access: true,
                    },
                    Range::from_located(expr),
                ));
            }
        }
    }
    None
}
