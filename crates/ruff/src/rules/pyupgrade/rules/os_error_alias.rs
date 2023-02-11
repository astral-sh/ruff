use itertools::Itertools;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Excepthandler, ExcepthandlerKind, Expr, ExprKind, Located};

use crate::ast::helpers::compose_call_path;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct OSErrorAlias {
        pub name: Option<String>,
    }
);
impl AlwaysAutofixableViolation for OSErrorAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Replace aliased errors with `OSError`")
    }

    fn autofix_title(&self) -> String {
        let OSErrorAlias { name } = self;
        match name {
            None => "Replace with builtin `OSError`".to_string(),
            Some(name) => format!("Replace `{name}` with builtin `OSError`"),
        }
    }
}

const ERROR_NAMES: &[&str] = &["EnvironmentError", "IOError", "WindowsError"];
const ERROR_MODULES: &[&str] = &["mmap", "select", "socket"];

fn corrected_name(checker: &Checker, original: &str) -> String {
    if ERROR_NAMES.contains(&original)
        && checker.is_builtin(original)
        && checker.is_builtin("OSError")
    {
        "OSError".to_string()
    } else {
        original.to_string()
    }
}

fn get_before_replace(elts: &[Expr]) -> Vec<String> {
    elts.iter()
        .map(|elt| {
            if let ExprKind::Name { id, .. } = &elt.node {
                id.to_string()
            } else {
                String::new()
            }
        })
        .collect()
}

fn check_module(checker: &Checker, expr: &Expr) -> (Vec<String>, Vec<String>) {
    let mut replacements: Vec<String> = vec![];
    let mut before_replace: Vec<String> = vec![];
    if let Some(call_path) = checker.resolve_call_path(expr) {
        for module in ERROR_MODULES.iter() {
            if call_path.as_slice() == [module, "error"] {
                replacements.push("OSError".to_string());
                before_replace.push(format!("{module}.error"));
                break;
            }
        }
    }
    (replacements, before_replace)
}

fn handle_name_or_attribute(
    checker: &Checker,
    item: &Expr,
    replacements: &mut Vec<String>,
    before_replace: &mut Vec<String>,
) {
    match &item.node {
        ExprKind::Name { id, .. } => {
            let (temp_replacements, temp_before_replace) = check_module(checker, item);
            replacements.extend(temp_replacements);
            before_replace.extend(temp_before_replace);
            if replacements.is_empty() {
                let new_name = corrected_name(checker, id);
                replacements.push(new_name);
                before_replace.push(id.to_string());
            }
        }
        ExprKind::Attribute { .. } => {
            let (temp_replacements, temp_before_replace) = check_module(checker, item);
            replacements.extend(temp_replacements);
            before_replace.extend(temp_before_replace);
        }
        _ => (),
    }
}

/// Handles one block of an except (use a loop if there are multiple blocks)
fn handle_except_block(checker: &mut Checker, handler: &Located<ExcepthandlerKind>) {
    let ExcepthandlerKind::ExceptHandler { type_, .. } = &handler.node;
    let Some(error_handlers) = type_.as_ref() else {
        return;
    };
    // The first part creates list of all the exceptions being caught, and
    // what they should be changed to
    let mut replacements: Vec<String> = vec![];
    let mut before_replace: Vec<String> = vec![];
    match &error_handlers.node {
        ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
            handle_name_or_attribute(
                checker,
                error_handlers,
                &mut replacements,
                &mut before_replace,
            );
        }
        ExprKind::Tuple { elts, .. } => {
            before_replace = get_before_replace(elts);
            for elt in elts {
                match &elt.node {
                    ExprKind::Name { id, .. } => {
                        let new_name = corrected_name(checker, id);
                        replacements.push(new_name);
                    }
                    ExprKind::Attribute { .. } => {
                        let (new_replacements, new_before_replace) = check_module(checker, elt);
                        replacements.extend(new_replacements);
                        before_replace.extend(new_before_replace);
                    }
                    _ => (),
                }
            }
        }
        _ => return,
    }
    replacements = replacements
        .iter()
        .unique()
        .map(std::string::ToString::to_string)
        .collect();
    before_replace = before_replace
        .iter()
        .filter(|x| !x.is_empty())
        .map(std::string::ToString::to_string)
        .collect();

    // This part checks if there are differences between what there is and
    // what there should be. Where differences, the changes are applied
    handle_making_changes(checker, error_handlers, &before_replace, &replacements);
}

fn handle_making_changes(
    checker: &mut Checker,
    target: &Expr,
    before_replace: &[String],
    replacements: &[String],
) {
    if before_replace != replacements && !replacements.is_empty() {
        let mut final_str: String;
        if replacements.len() == 1 {
            final_str = replacements.get(0).unwrap().to_string();
        } else {
            final_str = replacements.join(", ");
            final_str.insert(0, '(');
            final_str.push(')');
        }
        let mut diagnostic = Diagnostic::new(
            OSErrorAlias {
                name: compose_call_path(target),
            },
            Range::from_located(target),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.amend(Fix::replacement(
                final_str,
                target.location,
                target.end_location.unwrap(),
            ));
        }
        checker.diagnostics.push(diagnostic);
    }
}

pub trait OSErrorAliasChecker {
    fn check_error(&self, checker: &mut Checker)
    where
        Self: Sized;
}

impl OSErrorAliasChecker for &Vec<Excepthandler> {
    fn check_error(&self, checker: &mut Checker) {
        for handler in self.iter() {
            handle_except_block(checker, handler);
        }
    }
}

impl OSErrorAliasChecker for &Box<Expr> {
    fn check_error(&self, checker: &mut Checker) {
        let mut replacements: Vec<String> = vec![];
        let mut before_replace: Vec<String> = vec![];
        match &self.node {
            ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
                handle_name_or_attribute(checker, self, &mut replacements, &mut before_replace);
            }
            _ => return,
        }
        handle_making_changes(checker, self, &before_replace, &replacements);
    }
}

impl OSErrorAliasChecker for &Expr {
    fn check_error(&self, checker: &mut Checker) {
        let mut replacements: Vec<String> = vec![];
        let mut before_replace: Vec<String> = vec![];
        let change_target: &Expr;
        match &self.node {
            ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
                change_target = self;
                handle_name_or_attribute(checker, self, &mut replacements, &mut before_replace);
            }
            ExprKind::Call { func, .. } => {
                change_target = func;
                match &func.node {
                    ExprKind::Name { .. } | ExprKind::Attribute { .. } => {
                        handle_name_or_attribute(
                            checker,
                            func,
                            &mut replacements,
                            &mut before_replace,
                        );
                    }
                    _ => return,
                }
            }
            _ => return,
        }
        handle_making_changes(checker, change_target, &before_replace, &replacements);
    }
}

/// UP024
pub fn os_error_alias<U: OSErrorAliasChecker>(checker: &mut Checker, handlers: &U) {
    handlers.check_error(checker);
}
