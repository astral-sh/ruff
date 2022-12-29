use rustpython_ast::{Excepthandler, ExcepthandlerKind, ExprKind, Located};
use itertools::Itertools;

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
    elts.iter()
        .map(|elt| {
            if let ExprKind::Name { id, .. } = &elt.node {
                id.to_string()
            } else {
                "".to_string()
            }
        })
        .collect()
}

fn check_module(checker: &Checker, expr: &Located<ExprKind>) -> (Vec<String>, Vec<String>) {
    let mut replacements: Vec<String> = vec![];
    let mut before_replace: Vec<String> = vec![];
    for module in ERROR_MODULES.iter() {
        if match_module_member(
            &expr,
            module,
            "error",
            &checker.from_imports,
            &checker.import_aliases,
        ) {
            replacements.push("OSError".to_string());
            before_replace.push(format!("{}.error", module));
            break;
        }
    }
    (replacements, before_replace)
}

/// Handles one block of an except (use a loop if there are multile blocks)
fn handle_except_block(checker: &mut Checker, handler: &Located<ExcepthandlerKind>) {
    let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
    let error_handlers = match type_.as_ref() {
        None => return,
        Some(expr) => expr,
    };
    // The first part creates list of all the exceptions being caught, and
    // what they should be changed to
    let mut replacements: Vec<String> = vec![];
    let mut before_replace: Vec<String>;
    match &error_handlers.node {
        ExprKind::Name { id, .. } => {
            (replacements, before_replace) = check_module(checker, error_handlers);
            if replacements.is_empty() {
                let new_name = get_correct_name(id);
                replacements.push(new_name);
                before_replace.push(id.to_string());
            }
        }
        ExprKind::Attribute { .. } => {
            (replacements, before_replace) = check_module(checker, error_handlers);
        }
        ExprKind::Tuple { elts, .. } => {
            before_replace = get_before_replace(elts);
            for elt in elts {
                match &elt.node {
                    ExprKind::Name { id, .. } => {
                        let new_name = get_correct_name(id);
                        replacements.push(new_name);
                    },
                    ExprKind::Attribute { .. } => {
                        let (new_replacements, new_before_replace) = check_module(checker, elt);
                        replacements.extend(new_replacements);
                        before_replace.extend(new_before_replace);
                    },
                    _ => ()
                }
            }
        }
        _ => return,
    }
    replacements = replacements
        .iter()
        .unique()
        .map(|x| x.to_string())
        .collect();
    before_replace = before_replace.iter().filter(|x| !x.is_empty()).map(|x| x.to_string()).collect();

    // This part checks if there are differences between what there is and
    // what there should be. Where differences, the changes are applied
    handle_making_changes(checker, error_handlers, before_replace, replacements);
}

fn handle_making_changes(checker: &mut Checker, target: &Located<ExprKind>, before_replace: Vec<String>, replacements: Vec<String>)  {
    if before_replace != replacements && replacements.len() > 0 {
        let range = Range::new(
            target.location,
            target.end_location.unwrap(),
        );
        let contents = checker.locator.slice_source_code_range(&range);
        // Pyyupgrade does not want imports changed if a module only is
        // surrounded by parentheses. For example: `except mmap.error:`
        // would be changed, but: `(mmap).error:` would not. One issue with
        // this implementation is that any valid changes will also be
        // ignored. Let me know if you want me to go with a more
        // complicated solution that avoids this.
        if contents.contains(").") {
            return;
        }
        let mut final_str: String;
        if replacements.len() == 1 {
            final_str = replacements.get(0).unwrap().to_string();
        } else {
            final_str = replacements.join(", ");
            final_str.insert(0, '(');
            final_str.push(')');
        }
        let mut check = Check::new(CheckKind::OSErrorAlias, range);
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

// This is a hacky way to handle the different variable types we get since
// raise and try are very different. Would love input on a cleaner way
pub trait OSErrorAliasChecker {
    fn check_error(&self, checker: &mut Checker)
    where
        Self: Sized;
}

impl OSErrorAliasChecker for &Vec<Excepthandler> {
    fn check_error(&self, checker: &mut Checker) {
    // Each separate except block is a separate error and fix
        for handler in self.clone().iter() {
            handle_except_block(checker, handler);
        }
    }
}

impl OSErrorAliasChecker for &Box<Located<ExprKind>> {
    fn check_error(&self, checker: &mut Checker) {
        let mut replacements: Vec<String>;
        let mut before_replace: Vec<String>;
        match &self.node {
            ExprKind::Name { id, .. } => {
                (replacements, before_replace) = check_module(checker, &self);
                if replacements.is_empty() {
                    let new_name = get_correct_name(&id);
                    replacements.push(new_name);
                    before_replace.push(id.to_string());
                }
            }
            ExprKind::Attribute { .. } => {
                (replacements, before_replace) = check_module(checker, &self);
            },
            _ => return        
        }
        handle_making_changes(checker, self, before_replace, replacements);
    }
}

impl OSErrorAliasChecker for &Located<ExprKind> {
    fn check_error(&self, checker: &mut Checker) {
        let mut replacements: Vec<String>;
        let mut before_replace: Vec<String>;
        let change_target: &Located<ExprKind>;
        match &self.node {
            ExprKind::Name { id, .. } => {
                change_target = &self;
                (replacements, before_replace) = check_module(checker, &self);
                if replacements.is_empty() {
                    let new_name = get_correct_name(&id);
                    replacements.push(new_name);
                    before_replace.push(id.to_string());
                }
            }
            ExprKind::Attribute { .. } => {
                change_target = &self;
                (replacements, before_replace) = check_module(checker, &self);
            },
            ExprKind::Call { func, args, keywords } => {
                change_target = &func;
                match &func.node {
                    ExprKind::Name { id, .. } => {
                        (replacements, before_replace) = check_module(checker, &func);
                        if replacements.is_empty() {
                            let new_name = get_correct_name(&id);
                            replacements.push(new_name);
                            before_replace.push(id.to_string());
                        }
                    }
                    ExprKind::Attribute { .. } => {
                        (replacements, before_replace) = check_module(checker, &func);
                    },
                    _ => return
                }
                println!("{:?}", func);
            }
            _ => return        
        }
        handle_making_changes(checker, change_target, before_replace, replacements);
    }
}

/// UP024
pub fn os_error_alias<U: OSErrorAliasChecker>(checker: &mut Checker, handlers: U) {
    handlers.check_error(checker);
}
