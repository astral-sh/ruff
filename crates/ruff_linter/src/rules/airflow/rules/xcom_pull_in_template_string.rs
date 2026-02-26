use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::is_airflow_builtin_or_provider;
use crate::{FixAvailability, Violation};

/// ## What it does
/// Checks for Airflow operator keyword arguments that use a Jinja template
/// string containing a single `xcom_pull` call to retrieve another task's
/// output.
///
/// ## Why is this bad?
/// Using `{{ ti.xcom_pull(task_ids='some_task') }}` as a string template to
/// access the output of an upstream task is less readable and more
/// error-prone than using the `.output` attribute on the task object
/// directly. The `.output` attribute provides better IDE support and makes
/// task dependencies more explicit.
///
/// ## Example
/// ```python
/// from airflow.operators.python import PythonOperator
///
///
/// task_1 = PythonOperator(task_id="task_1", python_callable=my_func)
/// task_2 = PythonOperator(
///     task_id="task_2",
///     op_args="{{ ti.xcom_pull(task_ids='task_1') }}",
/// )
/// ```
///
/// Use instead:
/// ```python
/// from airflow.operators.python import PythonOperator
///
///
/// task_1 = PythonOperator(task_id="task_1", python_callable=my_func)
/// task_2 = PythonOperator(
///     task_id="task_2",
///     op_args=task_1.output,
/// )
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct AirflowXcomPullInTemplateString {
    task_id: String,
}

impl Violation for AirflowXcomPullInTemplateString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let AirflowXcomPullInTemplateString { task_id } = self;
        format!(
            "Use the `.output` attribute on the task object for \"{task_id}\" instead of `xcom_pull` in a template string"
        )
    }

    fn fix_title(&self) -> Option<String> {
        let AirflowXcomPullInTemplateString { task_id } = self;
        Some(format!("Replace with `{task_id}.output`"))
    }
}

/// AIR004
pub(crate) fn xcom_pull_in_template_string(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    // Check if this is a call to an Airflow operator or sensor.
    if !checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            is_airflow_builtin_or_provider(qualified_name.segments(), "operators", "Operator")
                || is_airflow_builtin_or_provider(qualified_name.segments(), "sensors", "Sensor")
        })
    {
        return;
    }

    // Check keyword arguments for xcom_pull template strings.
    for keyword in &*call.arguments.keywords {
        // Skip non-string-literal values.
        let Some(string_literal) = keyword.value.as_string_literal_expr() else {
            continue;
        };

        let string_value = string_literal.value.to_str();

        if let Some(task_id) = parse_xcom_pull_template(string_value) {
            let mut diagnostic = checker.report_diagnostic(
                AirflowXcomPullInTemplateString {
                    task_id: task_id.clone(),
                },
                keyword.value.range(),
            );

            // If the task_id matches a variable in scope, provide an unsafe fix
            // replacing the template string with `<variable>.output`.
            if checker.semantic().lookup_symbol(&task_id).is_some() {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    format!("{task_id}.output"),
                    keyword.value.range(),
                )));
            }
        }
    }
}

/// Parse a Jinja template string to extract a single `xcom_pull` task ID.
///
/// Returns the task ID if the entire string is a single `{{ ti.xcom_pull(task_ids='...') }}`
/// or `{{ task_instance.xcom_pull(task_ids='...') }}` template. Returns `None` if the string
/// contains other content, multiple task IDs, or additional keyword arguments.
fn parse_xcom_pull_template(s: &str) -> Option<String> {
    let s = s.trim();
    let s = s.strip_prefix("{{")?;
    let s = s.strip_suffix("}}")?;
    let s = s.trim();

    // Strip the object and method call prefix.
    let s = s
        .strip_prefix("ti.")
        .or_else(|| s.strip_prefix("task_instance."))?;
    let s = s.strip_prefix("xcom_pull(")?;
    let s = s.strip_suffix(')')?;
    let s = s.trim();

    // Handle keyword argument: `task_ids=` or `task_id=`.
    // Check `task_ids` first since `task_id` is a prefix of it.
    let s = if let Some(rest) = s.strip_prefix("task_ids") {
        rest.trim_start().strip_prefix('=')?.trim_start()
    } else if let Some(rest) = s.strip_prefix("task_id") {
        rest.trim_start().strip_prefix('=')?.trim_start()
    } else {
        // Positional argument.
        s
    };

    // Extract the quoted string value.
    let (quote_char, inner) = if let Some(rest) = s.strip_prefix('\'') {
        ('\'', rest)
    } else if let Some(rest) = s.strip_prefix('"') {
        ('"', rest)
    } else {
        return None;
    };

    // Find the closing quote.
    let end = inner.find(quote_char)?;
    let task_id = &inner[..end];
    let remaining = inner[end + 1..].trim();

    // Ensure nothing else follows (no additional arguments like `key=...`).
    if !remaining.is_empty() {
        return None;
    }

    if task_id.is_empty() {
        return None;
    }

    Some(task_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_xcom_pull_template;

    #[test]
    fn test_basic_ti_xcom_pull() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_task_instance_xcom_pull() {
        assert_eq!(
            parse_xcom_pull_template("{{ task_instance.xcom_pull(task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_positional_argument() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull('my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_double_quotes() {
        assert_eq!(
            parse_xcom_pull_template(r#"{{ ti.xcom_pull(task_ids="my_task") }}"#),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_no_spaces_in_braces() {
        assert_eq!(
            parse_xcom_pull_template("{{ti.xcom_pull(task_ids='my_task')}}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_extra_whitespace() {
        assert_eq!(
            parse_xcom_pull_template("{{  ti.xcom_pull( task_ids = 'my_task' )  }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_task_id_singular_keyword() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_id='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_rejects_mixed_content() {
        assert_eq!(
            parse_xcom_pull_template("echo {{ ti.xcom_pull(task_ids='my_task') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_additional_arguments() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task', key='my_key') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_empty_task_id() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_no_template() {
        assert_eq!(parse_xcom_pull_template("just a string"), None);
    }

    #[test]
    fn test_rejects_list_task_ids() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['a', 'b']) }}"),
            None
        );
    }
}
