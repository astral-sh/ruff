use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{self as ast, Expr, StmtFunctionDef};
use ruff_python_semantic::{Imported, Modules, ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `Variable.get()` calls outside of Airflow task execution
/// context (i.e., outside `@task`-decorated functions and operator
/// `execute()` methods).
///
/// ## Why is this bad?
/// Calling `Variable.get()` at module level or in operator constructor
/// arguments causes a database query every time the DAG file is parsed
/// by the scheduler. This can degrade DAG parsing performance and, in
/// some cases, cause the DAG file to time out before it is fully parsed.
///
/// Instead, pass Airflow Variables to operators via Jinja templates
/// (`{{ var.value.my_var }}` or `{{ var.json.my_var }}`), which defer
/// the lookup until task execution.
///
/// `Variable.get()` inside `@task`-decorated functions and operator
/// `execute()` methods is fine because those only run during task
/// execution, not during DAG parsing.
///
/// ## Example
/// ```python
/// from airflow.sdk import Variable
/// from airflow.operators.bash import BashOperator
///
///
/// foo = Variable.get("foo")
/// BashOperator(task_id="bad", bash_command="echo $FOO", env={"FOO": foo})
/// ```
///
/// Use instead:
/// ```python
/// from airflow.operators.bash import BashOperator
///
///
/// BashOperator(
///     task_id="good",
///     bash_command="echo $FOO",
///     env={"FOO": "{{ var.value.foo }}"},
/// )
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct AirflowVariableGetOutsideTask;

impl Violation for AirflowVariableGetOutsideTask {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`Variable.get()` outside of a task; use Jinja templates or move into a `@task`-decorated function".to_string()
    }
}

/// AIR005
pub(crate) fn variable_get_outside_task(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return;
    };

    if !is_variable_get(func, checker.semantic()) {
        return;
    }

    if !is_dag_file(checker.semantic()) {
        return;
    }

    if in_task_execution_context(checker.semantic()) {
        return;
    }

    checker.report_diagnostic(AirflowVariableGetOutsideTask, expr.range());
}

/// Returns `true` if the file imports `DAG` or `dag` from airflow,
/// indicating it is a DAG definition file.
fn is_dag_file(semantic: &SemanticModel) -> bool {
    semantic.global_scope().binding_ids().any(|binding_id| {
        let binding = semantic.binding(binding_id);
        binding.as_any_import().is_some_and(|import| {
            matches!(
                import.qualified_name().segments(),
                ["airflow", .., "DAG" | "dag"]
            )
        })
    })
}

/// Returns `true` if `func` resolves to `Variable.get`.
fn is_variable_get(func: &Expr, semantic: &SemanticModel) -> bool {
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                ["airflow", "models" | "sdk", .., "Variable", "get"]
            )
        })
}

/// Returns `true` if the current location is inside a `@task`-decorated function
/// or an `execute()` method on an Airflow operator subclass.
fn in_task_execution_context(semantic: &SemanticModel) -> bool {
    semantic
        .current_statements()
        .find_map(|stmt| stmt.as_function_def_stmt())
        .is_some_and(|function_def| {
            is_airflow_task(function_def, semantic)
                || is_execute_method_on_operator(function_def, semantic)
        })
}

/// Returns `true` if the function is decorated with `@task` or `@task.<something>`.
fn is_airflow_task(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        let expr = map_callable(&decorator.expression);

        // Match `@task` directly
        if semantic
            .resolve_qualified_name(expr)
            .is_some_and(|qn| matches!(qn.segments(), ["airflow", "decorators", "task"]))
        {
            return true;
        }

        // Match `@task.<variant>` (e.g., @task.branch, @task.short_circuit)
        if let Expr::Attribute(ast::ExprAttribute { value, .. }) = expr {
            if semantic
                .resolve_qualified_name(value)
                .is_some_and(|qn| matches!(qn.segments(), ["airflow", "decorators", "task"]))
            {
                return true;
            }
        }

        false
    })
}

/// Returns `true` if the function is named `execute` and is defined inside a
/// class that inherits from an Airflow operator.
fn is_execute_method_on_operator(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    if function_def.name.as_str() != "execute" {
        return false;
    }

    let Some(parent_scope) = semantic.first_non_type_parent_scope(semantic.current_scope()) else {
        return false;
    };

    let ScopeKind::Class(class_def) = parent_scope.kind else {
        return false;
    };

    class_def.bases().iter().any(|base| {
        semantic.resolve_qualified_name(base).is_some_and(|qn| {
            matches!(
                qn.segments(),
                ["airflow", "models" | "sdk", .., "BaseOperator"]
            )
        })
    })
}
