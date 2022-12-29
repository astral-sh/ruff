use itertools::Itertools;

use rustpython_ast::{Located, Excepthandler, ExcepthandlerKind, ExprKind};

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
        if let ExprKind::Name{ id, .. } = &elt.node {
            id.to_string()
        } else {
            "".to_string()
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
                for elt in elts {
                    if let ExprKind::Name{ id, .. } = &elt.node {
                        let new_name = get_correct_name(id);
                        replacements.push(new_name);
                    }
                }
            },
            _ => return,
        }
        replacements = replacements.iter().unique().map(|x| x.to_string()).collect();
        if before_replace != replacements {
            println!("Before: {:?}", before_replace);
            println!("Replacements: {:?}\n", replacements);
            let mut final_str: String;
            let message_str: String;
            if replacements.len() == 1 {
                final_str = replacements.get(0).unwrap().to_string();
                message_str = final_str.clone();
            } else {
                final_str = replacements.join(", ");
                message_str = final_str.clone();
                final_str.insert(0, '(');
                final_str.push(')');
            }
            let range = Range::new(error_handlers.location, error_handlers.end_location.unwrap());
            let mut check = Check::new(CheckKind::OSErrorAlias(message_str), range);
            if checker.patch(check.kind.code()) {
                check.amend(Fix::replacement(
                    final_str,
                    range.location,
                    range.end_location,
                ));
            }
            checker.add_check(check);
        }
    }
}
