use rustpython_parser::ast;
use rustpython_parser::ast::{Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks that the task variable name is the same as the task_id.
///
/// ## Why is this bad?
/// For consistency, the task variable name should be the same as the task_id.
/// This makes it easier for you and others to understand the code.
///
/// ## Example
/// ```python
/// incorrect_name = SomeOperator(task_id="my_task")
/// async def fetch():
/// ```
///
/// Use instead:
/// ```python
/// my_task = SomeOperator(task_id="my_task")
/// ```
#[violation]
pub struct TaskVariableNameNotTaskId;

impl Violation for TaskVariableNameNotTaskId {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Task variable name should be the same as the task_id")
    }
}

/// AIR001
pub(crate) fn task_variable_name(
    checker: &mut Checker,
    targets: &[Expr],
    value: &Expr,
) -> Option<Diagnostic> {
    // if we have more than one target, we can't do anything
    if targets.len() != 1 {
        return None;
    }

    let target = &targets[0];
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return None;
    };

    // if the value is not a call, we can't do anything
    let Expr::Call(call) = value else { return None };

    // if the function doesn't come from airflow, we can't do anything
    let func_name = match call.func.as_name_expr() {
        Some(name) => name.id.as_str(),
        None => return None,
    };
    let fully_qualified_func_name = match checker.semantic_model().find_binding(func_name) {
        Some(call_path) => match call_path.kind.as_from_importation() {
            Some(from_importation) => &from_importation.full_name,
            None => return None,
        },
        None => return None,
    };

    if !fully_qualified_func_name.starts_with("airflow.") {
        return None;
    }

    // if the call doesn't have a task_id, don't do anything
    let Some(task_id_arg) = call.keywords.iter().find(|keyword| match &keyword.arg {
            Some(arg_name) => arg_name == "task_id",
            _ => false,
        }) else { return None };

    // get the task_id value
    let task_id_arg_value = match &task_id_arg.value {
        Expr::Constant(constant) => match constant.value.as_str() {
            Some(string) => string,
            None => return None,
        },
        _ => return None,
    };

    // if the target name is the same as the task_id, no violation
    if &id.to_string() == task_id_arg_value {
        return None;
    }

    // if the target name is not the same as the task_id, violation
    Some(Diagnostic::new(TaskVariableNameNotTaskId, target.range()))
}
