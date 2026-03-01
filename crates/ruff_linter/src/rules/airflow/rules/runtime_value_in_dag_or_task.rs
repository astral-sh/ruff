use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, ExprCall, InterpolatedStringElement};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::is_airflow_builtin_or_provider;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for calls to runtime-varying functions (such as `datetime.now()`)
/// used as arguments in Airflow DAG or task constructors.
///
/// ## Why is this bad?
/// Using runtime-varying values as arguments to DAG or task constructors
/// causes the serialized DAG hash to change on every parse, creating
/// infinite DAG versions in the `dag_version` and `serialized_dag` tables.
/// This leads to unbounded database growth and can eventually cause
/// out-of-memory conditions.
///
/// ## Example
/// ```python
/// from datetime import datetime
///
/// from airflow import DAG
///
/// dag = DAG(dag_id="my_dag", start_date=datetime.now())
/// ```
///
/// Use instead:
/// ```python
/// from datetime import datetime
///
/// from airflow import DAG
///
/// dag = DAG(dag_id="my_dag", start_date=datetime(2024, 1, 1))
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.11")]
pub(crate) struct Airflow3DagDynamicValue {
    function_name: String,
}

impl Violation for Airflow3DagDynamicValue {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3DagDynamicValue { function_name } = self;
        format!(
            "`{function_name}()` produces a value that changes at runtime; using it in a DAG or task argument causes infinite DAG version creation"
        )
    }
}

/// AIR304
pub(crate) fn airflow_3_dag_dynamic_value(checker: &Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(&call.func) else {
        return;
    };

    if !is_dag_or_task_constructor(&qualified_name) {
        return;
    }

    for keyword in &call.arguments.keywords {
        if let Some((expr, name)) = find_runtime_varying_call(&keyword.value, checker.semantic()) {
            checker.report_diagnostic(
                Airflow3DagDynamicValue {
                    function_name: name.to_string(),
                },
                expr.range(),
            );
        }
    }
}

/// Check if the qualified name refers to a DAG constructor, `@dag` decorator,
/// operator, sensor, or `@task` decorator.
fn is_dag_or_task_constructor(qualified_name: &QualifiedName) -> bool {
    let segments = qualified_name.segments();
    matches!(segments, ["airflow", .., "DAG" | "dag"])
        || matches!(segments, ["airflow", "decorators" | "sdk", "task"])
        || is_airflow_builtin_or_provider(segments, "operators", "Operator")
        || is_airflow_builtin_or_provider(segments, "sensors", "Sensor")
}

/// Recursively check an expression for calls to known runtime-varying functions.
/// Returns the call expression and a display name (e.g., `"datetime.now"`) if found.
fn find_runtime_varying_call<'a>(
    expr: &'a Expr,
    semantic: &SemanticModel,
) -> Option<(&'a Expr, &'static str)> {
    match expr {
        Expr::Call(ExprCall { func, .. }) => {
            if let Some(qualified_name) = semantic.resolve_qualified_name(func) {
                let name = match qualified_name.segments() {
                    ["datetime", "datetime", "now"] => Some("datetime.now"),
                    ["datetime", "datetime", "utcnow"] => Some("datetime.utcnow"),
                    ["datetime", "datetime", "today"] => Some("datetime.today"),
                    ["datetime", "date", "today"] => Some("date.today"),
                    ["pendulum", "now"] => Some("pendulum.now"),
                    ["pendulum", "today"] => Some("pendulum.today"),
                    ["pendulum", "yesterday"] => Some("pendulum.yesterday"),
                    ["pendulum", "tomorrow"] => Some("pendulum.tomorrow"),
                    ["time", "time"] => Some("time.time"),
                    ["uuid", "uuid1"] => Some("uuid.uuid1"),
                    ["uuid", "uuid4"] => Some("uuid.uuid4"),
                    ["random", "random"] => Some("random.random"),
                    ["random", "randint"] => Some("random.randint"),
                    ["random", "choice"] => Some("random.choice"),
                    ["random", "uniform"] => Some("random.uniform"),
                    ["random", "randrange"] => Some("random.randrange"),
                    ["random", "sample"] => Some("random.sample"),
                    ["random", "getrandbits"] => Some("random.getrandbits"),
                    _ => None,
                };
                if let Some(name) = name {
                    return Some((expr, name));
                }
            }
            None
        }
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            find_runtime_varying_call(left, semantic)
                .or_else(|| find_runtime_varying_call(right, semantic))
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
            find_runtime_varying_call(operand, semantic)
        }
        Expr::Dict(ast::ExprDict { items, .. }) => items
            .iter()
            .find_map(|item| find_runtime_varying_call(&item.value, semantic)),
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => elts
            .iter()
            .find_map(|elt| find_runtime_varying_call(elt, semantic)),
        Expr::FString(ast::ExprFString { value, .. }) => value.elements().find_map(|element| {
            if let InterpolatedStringElement::Interpolation(interpolation) = element {
                find_runtime_varying_call(&interpolation.expression, semantic)
            } else {
                None
            }
        }),
        _ => None,
    }
}
