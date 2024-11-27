use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::Expr;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for usage of deprecated SubDag functionality in Airflow.
///
/// ## Why is this bad?
/// SubDags have been removed in Airflow 3.0 in favor of TaskGroups.
/// Code using SubDags needs to be updated to use TaskGroups instead.
///
/// ## Example
/// ```python
/// from airflow.operators.subdag import SubDagOperator
///
/// # Invalid: using deprecated SubDag
/// task = SubDagOperator(
///     task_id="subdag_task",
///     subdag=some_subdag,
/// )
/// ```
///
/// Use instead:
/// ```python
/// from airflow.utils.task_group import TaskGroup
///
/// # Valid: using TaskGroup
/// with TaskGroup(group_id="my_task_group") as task_group:
///     task = SomeOperator(...)
/// ```
#[violation]
pub struct AirflowDeprecatedSubDag;

impl Violation for AirflowDeprecatedSubDag {
    #[derive_message_formats]
    fn message(&self) -> String {
        "SubDags have been removed in Airflow 3.0, use TaskGroups instead".to_string()
    }
}

/// AIR304
pub(crate) fn subdag_check(checker: &mut Checker, value: &Expr) {
    // If the value is not a call, we can't do anything.
    let ast::Expr::Call(ast::ExprCall { func, .. }) = value else {
        return;
    };

    // If we haven't seen any airflow imports, we can't do anything.
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // Check if it's a SubDagOperator import or usage
    if let Some(qualified_name) = checker.semantic().resolve_qualified_name(func) {
        let segments = qualified_name.segments();
        if segments.contains(&"operators") && segments.contains(&"subdag")
            // Also check for direct subdag imports
            || segments.contains(&"SubDagOperator")
        {
            checker
                .diagnostics
                .push(Diagnostic::new(AirflowDeprecatedSubDag, func.range()));
        }
    }
}
