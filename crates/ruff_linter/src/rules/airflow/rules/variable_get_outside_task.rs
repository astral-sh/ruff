use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, StmtFunctionDef};
use ruff_python_semantic::analyze::class::any_qualified_base_class;
use ruff_python_semantic::{Imported, Modules, ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::is_airflow_task;

/// ## What it does
/// Checks for `Variable.get()` calls outside of Airflow task execution
/// context (i.e., outside `@task`-decorated functions and operator
/// `execute()` methods).
///
/// ## Why is this bad?
/// Calling `Variable.get()` at module level or in operator constructor
/// arguments causes a database query every time the Dag file is parsed
/// by the scheduler. This can degrade Dag parsing performance and, in
/// some cases, cause the Dag file to time out before it is fully parsed.
///
/// Instead, pass Airflow Variables to operators via Jinja templates
/// (`{{ var.value.my_var }}` or `{{ var.json.my_var }}`), which defer
/// the lookup until task execution.
///
/// `Variable.get()` inside `@task`-decorated functions and operator
/// `execute()` methods is fine because those only run during task
/// execution, not during Dag parsing.
///
/// Note that this rule may produce false positives for helper functions
/// that are only invoked at task execution time (e.g., passed as
/// `python_callable` to `PythonOperator`). In such cases, suppress the
/// diagnostic with `# noqa: AIR003`.
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
#[violation_metadata(preview_since = "0.15.6")]
pub(crate) struct AirflowVariableGetOutsideTask {
    in_function: bool,
}

impl Violation for AirflowVariableGetOutsideTask {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`Variable.get()` outside of a task".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        if self.in_function {
            Some("Move into a `@task`-decorated function".to_string())
        } else {
            Some("Use Jinja templates instead".to_string())
        }
    }
}

/// AIR003
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

    let in_function = matches!(
        checker.semantic().current_scope().kind,
        ScopeKind::Function(_) | ScopeKind::Lambda(_)
    );

    checker.report_diagnostic(AirflowVariableGetOutsideTask { in_function }, expr.range());
}

/// Returns `true` if the file imports `DAG` or `dag` from airflow, which
/// indicates it is a Dag definition file.
fn is_dag_file(semantic: &SemanticModel) -> bool {
    semantic.global_scope().binding_ids().any(|binding_id| {
        semantic
            .binding(binding_id)
            .as_any_import()
            .is_some_and(|import| {
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
/// or a task-execution-time method on an Airflow operator subclass.
fn in_task_execution_context(semantic: &SemanticModel) -> bool {
    semantic
        .current_statements()
        .find_map(|stmt| stmt.as_function_def_stmt())
        .is_some_and(|function_def| {
            is_airflow_task(function_def, semantic)
                || is_operator_task_method(function_def, semantic)
        })
}

/// Returns `true` if the function is a task-execution-time method (`execute`,
/// `pre_execute`, or `post_execute`) defined inside a class that inherits from
/// an Airflow operator.
///
/// This is similar to `helpers::is_method_in_subclass` but can't reuse it
/// directly because we're called from inside the function body (need to walk up
/// to the parent class scope), whereas `is_method_in_subclass` expects to already
/// be at the class scope.
fn is_operator_task_method(function_def: &StmtFunctionDef, semantic: &SemanticModel) -> bool {
    if !matches!(
        function_def.name.as_str(),
        "execute" | "pre_execute" | "post_execute"
    ) {
        return false;
    }

    let Some(parent_scope) = semantic.first_non_type_parent_scope(semantic.current_scope()) else {
        return false;
    };

    let ScopeKind::Class(class_def) = parent_scope.kind else {
        return false;
    };

    any_qualified_base_class(class_def, semantic, &|qn| {
        matches!(
            qn.segments(),
            ["airflow", "models" | "sdk", .., "BaseOperator"]
        )
    })
}
