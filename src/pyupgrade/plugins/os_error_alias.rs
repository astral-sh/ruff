use rustpython_ast::{Expr, Located, Excepthandler, ExcepthandlerKind, ExprKind};

use crate::ast::helpers::match_module_member;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

const ERROR_NAMES: &'static [&'static str] = &["EnvironmentError", "IOError", "WindowsError"];
const ERROR_MODULES: &'static [&'static str] = &["mmap", "select", "socket"];

fn get_correct_name(original: &str) -> String {
    if ERROR_NAMES.contains(&original) {
        return "OSError".to_string();
    } else {
        return original.to_string();
    }
}

fn get_before_replace(elts: &Vec<Located<ExprKind>>) -> Vec<String> {
    elts.iter().map(|elt| {
        match &elt.node {
            ExprKind::Name{ id, .. } => id.as_str().to_string(),
            _ => "".to_string(),
        }
    }).collect()
}

/// UP024
pub fn os_error_alias(checker: &mut Checker, handlers: &Vec<Excepthandler>) {
    // Each separate except block is a separate error and fix
    for handler in handlers {
        let ExcepthandlerKind::ExceptHandler{ type_, .. } = &handler.node;
        let error_handlers = match type_.as_ref() {
            None => return,
            Some(expr) => expr,
        };
        let mut replacements: Vec<String> = vec![];
        let mut before_replace: Vec<String> = vec![];
        match &error_handlers.node {
            ExprKind::Name{ id, .. } => {
                let new_name = get_correct_name(id);
                replacements.push(new_name);
            },
            ExprKind::Tuple { elts, .. } => {
                before_replace = get_before_replace(elts);
                println!("Before: {:?}", before_replace);
                for id in elts {
                    let new_name = get_correct_name(id);
                    replacements.push(new_name);
                }
            },
            _ => return,
        }
    }
    /*
    if match_module_member(
        expr,
        "typing",
        "Text",
        &checker.from_imports,
        &checker.import_aliases,
    ) 
    let mut check = Check::new(CheckKind::TypingTextStrAlias, Range::from_located(expr));
    if checker.patch(check.kind.code()) {
        check.amend(Fix::replacement(
            "str".to_string(),
            expr.location,
            expr.end_location.unwrap(),
        ));
    }
    checker.add_check(check);
    */
}
