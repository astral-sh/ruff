use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{Expr, InterpolatedStringElement};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::airflow::helpers::is_airflow_builtin_or_provider;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for calls to runtime-varying functions (such as `datetime.now()`)
/// used as arguments in Airflow Dag or task constructors.
///
/// ## Why is this bad?
/// Using runtime-varying values as arguments to Dag or task constructors
/// causes the serialized Dag hash to change on every parse, creating
/// infinite Dag versions in the `dag_version` and `serialized_dag` tables.
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
#[violation_metadata(preview_since = "0.15.6", category = Category::Correctness)]
pub(crate) struct Airflow3DagDynamicValue {
    function_name: String,
}

impl Violation for Airflow3DagDynamicValue {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow3DagDynamicValue { function_name } = self;
        format!(
            "`{function_name}()` produces a value that changes at runtime; using it in a Dag or task argument causes infinite Dag version creation"
        )
    }
}

/// AIR304
pub(crate) fn airflow_3_dag_dynamic_value(checker: &Checker, call: &ast::ExprCall<'_>) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    let Some(qualified_name) = checker.semantic().resolve_qualified_name(call.func) else {
        return;
    };

    if !is_dag_or_task_constructor(&qualified_name) {
        return;
    }

    for keyword in call.arguments.keywords() {
        if let Some((expr, name)) = find_runtime_varying_call(keyword.value(), checker.semantic()) {
            checker.report_diagnostic(
                Airflow3DagDynamicValue {
                    function_name: name.to_string(),
                },
                expr.range(),
            );
        }
    }
}

/// Check if the qualified name refers to a Dag constructor, `@dag` decorator,
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
///
/// The set of traversed expression variants mirrors [`any_over_expr`] in
/// `ruff_python_ast::helpers` so that no nested sub-expression is silently skipped.
fn find_runtime_varying_call<'a>(
    expr: flat::View<'a, Expr>,
    semantic: &SemanticModel,
) -> Option<(flat::View<'a, Expr>, &'static str)> {
    match expr.fields() {
        ast::Expr::Call(ast::ExprCall {
            func, arguments, ..
        }) => {
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
            // Recurse into the callee (catches method chains like `datetime.now().isoformat()`)
            // and into arguments (catches wrappers like `str(datetime.now())`).
            find_runtime_varying_call(func, semantic)
                .or_else(|| {
                    arguments
                        .args()
                        .iter()
                        .find_map(|arg| find_runtime_varying_call(arg, semantic))
                })
                .or_else(|| {
                    arguments
                        .keywords()
                        .iter()
                        .find_map(|kw| find_runtime_varying_call(kw.value(), semantic))
                })
        }
        ast::Expr::BoolOp(ast::ExprBoolOp { values, .. }) => values
            .iter()
            .find_map(|value| find_runtime_varying_call(value, semantic)),
        ast::Expr::Named(ast::ExprNamed { target, value, .. }) => {
            find_runtime_varying_call(target, semantic)
                .or_else(|| find_runtime_varying_call(value, semantic))
        }
        ast::Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            find_runtime_varying_call(left, semantic)
                .or_else(|| find_runtime_varying_call(right, semantic))
        }
        ast::Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => {
            find_runtime_varying_call(operand, semantic)
        }
        // Lambda defers execution — `lambda: datetime.now()` does not call
        // `now()` at DAG parse time, so traversing into the body would be a
        // false positive.
        ast::Expr::Lambda(_) => None,
        ast::Expr::If(ast::ExprIf {
            test, body, orelse, ..
        }) => find_runtime_varying_call(test, semantic)
            .or_else(|| find_runtime_varying_call(body, semantic))
            .or_else(|| find_runtime_varying_call(orelse, semantic)),
        ast::Expr::Dict(ast::ExprDict { items, .. }) => items.iter().find_map(|item| {
            item.key
                .and_then(|k| find_runtime_varying_call(k, semantic))
                .or_else(|| find_runtime_varying_call(item.value, semantic))
        }),
        ast::Expr::List(ast::ExprList { elts, .. })
        | ast::Expr::Tuple(ast::ExprTuple { elts, .. })
        | ast::Expr::Set(ast::ExprSet { elts, .. }) => elts
            .iter()
            .find_map(|elt| find_runtime_varying_call(elt, semantic)),
        ast::Expr::ListComp(ast::ExprListComp {
            elt, generators, ..
        })
        | ast::Expr::SetComp(ast::ExprSetComp {
            elt, generators, ..
        })
        | ast::Expr::Generator(ast::ExprGenerator {
            elt, generators, ..
        }) => find_runtime_varying_call(elt, semantic).or_else(|| {
            generators.iter().find_map(|generator| {
                find_runtime_varying_call(generator.target(), semantic)
                    .or_else(|| find_runtime_varying_call(generator.iter(), semantic))
                    .or_else(|| {
                        generator
                            .ifs()
                            .iter()
                            .find_map(|e| find_runtime_varying_call(e, semantic))
                    })
            })
        }),
        ast::Expr::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            ..
        }) => key
            .and_then(|key| find_runtime_varying_call(key, semantic))
            .or_else(|| find_runtime_varying_call(value, semantic))
            .or_else(|| {
                generators.iter().find_map(|generator| {
                    find_runtime_varying_call(generator.target(), semantic)
                        .or_else(|| find_runtime_varying_call(generator.iter(), semantic))
                        .or_else(|| {
                            generator
                                .ifs()
                                .iter()
                                .find_map(|e| find_runtime_varying_call(e, semantic))
                        })
                })
            }),
        ast::Expr::Await(ast::ExprAwait { value, .. })
        | ast::Expr::YieldFrom(ast::ExprYieldFrom { value, .. })
        | ast::Expr::Attribute(ast::ExprAttribute { value, .. })
        | ast::Expr::Starred(ast::ExprStarred { value, .. }) => {
            find_runtime_varying_call(value, semantic)
        }
        ast::Expr::Yield(ast::ExprYield { value, .. }) => {
            value.and_then(|v| find_runtime_varying_call(v, semantic))
        }
        ast::Expr::Compare(ast::ExprCompare { operands, .. }) => operands
            .iter()
            .find_map(|operand| find_runtime_varying_call(operand, semantic)),
        ast::Expr::FString(ast::ExprFString { value, .. }) => value
            .elements()
            .find_map(|element| find_runtime_in_interpolated_element(element, semantic)),
        ast::Expr::TString(ast::ExprTString { value, .. }) => value
            .elements()
            .find_map(|element| find_runtime_in_interpolated_element(element, semantic)),
        ast::Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            find_runtime_varying_call(value, semantic)
                .or_else(|| find_runtime_varying_call(slice, semantic))
        }
        ast::Expr::Slice(ast::ExprSlice {
            lower, upper, step, ..
        }) => lower
            .and_then(|v| find_runtime_varying_call(v, semantic))
            .or_else(|| upper.and_then(|v| find_runtime_varying_call(v, semantic)))
            .or_else(|| step.and_then(|v| find_runtime_varying_call(v, semantic))),
        ast::Expr::Name(_)
        | ast::Expr::StringLiteral(_)
        | ast::Expr::BytesLiteral(_)
        | ast::Expr::NumberLiteral(_)
        | ast::Expr::BooleanLiteral(_)
        | ast::Expr::NoneLiteral(_)
        | ast::Expr::EllipsisLiteral(_)
        | ast::Expr::IpyEscapeCommand(_) => None,
    }
}

/// Check an interpolated string element (f-string or t-string) for runtime-varying calls,
/// including format specifications.
fn find_runtime_in_interpolated_element<'a>(
    element: flat::View<'a, InterpolatedStringElement>,
    semantic: &SemanticModel,
) -> Option<(flat::View<'a, Expr>, &'static str)> {
    match element.classify() {
        flat::InterpolatedStringElementKind::Literal(_) => None,
        flat::InterpolatedStringElementKind::Interpolation(interpolation) => {
            find_runtime_varying_call(interpolation.expression(), semantic).or_else(|| {
                interpolation.format_spec().and_then(|spec| {
                    spec.elements()
                        .iter()
                        .find_map(|el| find_runtime_in_interpolated_element(el, semantic))
                })
            })
        }
    }
}
