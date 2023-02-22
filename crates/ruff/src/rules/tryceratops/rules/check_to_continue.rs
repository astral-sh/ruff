use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExprKind, Stmt, StmtKind};
use std::collections::HashMap;

define_violation!(
    /// ### What it does
    /// Checks for you are continuosuly checking whether you should continue
    ///
    /// ### Why is this bad?
    /// Continuously checking whether you should continue makes the function hard to read and
    /// follow up
    ///
    /// ### Example
    /// ```python
    /// def main_function():
    ///     process()
    ///     result = retrieve_result()
    ///
    ///     if not result:
    ///         return "Can't proceed"
    ///     details = result.maybe_get_details()
    ///     if len(details) == 0:
    ///         return "Impossible to finish without details"
    ///
    ///      more_work()
    ///      work_completed = finish_work()
    ///      if not work_completed:
    ///          return "Work failed"
    ///      send_details_with_result(result, details)
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def main_function():
    ///    process()
    ///    result = retrieve_result()
    ///    details = result.maybe_get_details()
    ///    more_work()
    ///    finish_work()
    ///    send_details_with_result(result, details)
    /// ```
    pub struct CheckToContinue;
);
impl Violation for CheckToContinue {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Allow the function to emit an error instead of continuously checking")
    }
}

fn get_bodies(stmt: &Stmt) -> &[Stmt] {
    match &stmt.node {
        StmtKind::FunctionDef { body, .. } => body,
        StmtKind::AsyncFunctionDef { body, .. } => body,
        StmtKind::ClassDef { body, .. } => body,
        StmtKind::For { body, .. } => body,
        StmtKind::AsyncFor { body, .. } => body,
        StmtKind::While { body, .. } => body,
        StmtKind::If { body, .. } => body,
        StmtKind::With { body, .. } => body,
        StmtKind::AsyncWith { body, .. } => body,
        StmtKind::Try { body, .. } => body,
        _ => &[],
    }
}

fn is_if_returning(node: &Stmt) -> bool {
    if let StmtKind::If { body, .. } = &node.node {
        for stmt in body {
            if let StmtKind::Return { .. } = &stmt.node {
                return true;
            }
        }
    }
    false
}

struct ContinueChecker {
    assignments_from_calls: HashMap<String, Stmt>,
}

impl ContinueChecker {
    fn new() -> Self {
        Self {
            assignments_from_calls: HashMap::new(),
        }
    }

    fn is_assigned_from_call(&mut self, node: &Stmt) -> bool {
        if let StmtKind::Assign { targets, value, .. } = &node.node {
            if let ExprKind::Call { .. } = &value.node {
                return true;
            }
            let first_target = match targets.first() {
                Some(item) => item,
                None => return false,
            };
            if let ExprKind::Name { id, .. } = &first_target.node {
                self.assignments_from_calls.remove(id);
            }
        }
        false
    }

    fn scan_assignments(&mut self, stmt: &Stmt) {
        let bodies = get_bodies(stmt);
        let raw_assignments: Vec<&Stmt> = bodies
            .iter()
            .filter(|s| self.is_assigned_from_call(s))
            .collect();

        for raw in raw_assignments {
            if let StmtKind::Assign { targets, .. } = &raw.node {
                let first_target = match targets.first() {
                    Some(item) => item,
                    None => continue,
                };
                if let ExprKind::Name { id, .. } = &first_target.node {
                    self.assignments_from_calls.insert(id.clone(), raw.clone());
                }
            }
        }
    }

    fn find_violations(&mut self, stmt: &Stmt) {
        let bodies = get_bodies(stmt);
        let mut ifs_stmt: Vec<Stmt> = vec![];
        for raw in bodies {
            if is_if_returning(raw) {
                ifs_stmt.push(raw.clone());
            }
        }
        if is_if_returning(stmt) {
            ifs_stmt.push(stmt.clone());
        }

        for if_stmt in ifs_stmt {
            if let StmtKind::If { test, .. } = &if_stmt.node {
                if let ExprKind::Name { id, ctx } = &test.node {
                    let assignment = self.assignments_from_calls.get(id);
                    if let Some(clean_assign) = assignment {}
                }
            }
        }
    }

    fn scan_deeper(&mut self, stmt: &Stmt, may_contain_violations: bool) {
        self.scan_assignments(stmt);
        if may_contain_violations {
            self.find_violations(stmt);
        }
    }
}

/// TRY100
pub fn check_to_continue(checker: &Checker) {
    let continue_check = ContinueChecker::new();
}
