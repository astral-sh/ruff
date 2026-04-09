use ruff_diagnostics::{Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_semantic::Modules;
use ruff_python_trivia::Cursor;
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
///
/// ## Fix safety
/// The fix is always unsafe because the variable in scope that matches the
/// task ID may not be the Airflow task object that produced the XCom value.
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

/// AIR201
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

    // Check all arguments for xcom_pull template strings. Any operator
    // argument can be a Jinja template field (determined by the operator's
    // `template_fields` attribute), so we check both positional and keyword
    // arguments.
    let arg_values = call
        .arguments
        .args
        .iter()
        .chain(call.arguments.keywords.iter().map(|kw| &kw.value));

    for arg_value in arg_values {
        let Some(string_literal) = arg_value.as_string_literal_expr() else {
            continue;
        };

        let string_value = string_literal.value.to_str();

        if let Some(task_id) = parse_xcom_pull_template(string_value) {
            let mut diagnostic = checker.report_diagnostic(
                AirflowXcomPullInTemplateString {
                    task_id: task_id.clone(),
                },
                arg_value.range(),
            );

            // If the task_id matches a variable in scope, provide an unsafe fix
            // replacing the template string with `<variable>.output`.
            if checker.semantic().lookup_symbol(&task_id).is_some() {
                diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                    format!("{task_id}.output"),
                    arg_value.range(),
                )));
            }
        }
    }
}

/// Parse a Jinja template string to extract a single `xcom_pull` task ID.
///
/// Returns the task ID if the entire string is a single `{{ ti.xcom_pull(task_ids='...') }}`
/// or `{{ task_instance.xcom_pull(task_ids='...') }}` template. The `task_ids` value may also
/// be wrapped in a list or tuple (e.g. `task_ids=['...']`), and an optional
/// `key='return_value'` argument is allowed (since it is the default).
/// Returns `None` if the string contains other content, multiple task IDs, or
/// non-default keyword arguments.
fn parse_xcom_pull_template(s: &str) -> Option<String> {
    let mut cursor = Cursor::new(s);
    eat_whitespace(&mut cursor);

    if !cursor.eat_char2('{', '{') {
        return None;
    }
    eat_whitespace(&mut cursor);

    let receiver = parse_identifier(&mut cursor)?;
    if receiver != "ti" && receiver != "task_instance" {
        return None;
    }
    eat_whitespace(&mut cursor);
    if !cursor.eat_char('.') {
        return None;
    }
    eat_whitespace(&mut cursor);
    if parse_identifier(&mut cursor)? != "xcom_pull" {
        return None;
    }

    eat_whitespace(&mut cursor);
    if !cursor.eat_char('(') {
        return None;
    }
    eat_whitespace(&mut cursor);

    // Handle keyword argument: `task_ids=` or `task_id=`.
    if let Some(identifier) = parse_identifier(&mut cursor) {
        if identifier != "task_ids" && identifier != "task_id" {
            return None;
        }
        eat_whitespace(&mut cursor);
        if !cursor.eat_char('=') {
            return None;
        }
        eat_whitespace(&mut cursor);
    }

    // Check for list or tuple wrapping: `['task']`, `('task')`, `('task',)`.
    let closing_bracket = if cursor.eat_char('[') {
        eat_whitespace(&mut cursor);
        Some(']')
    } else if cursor.eat_char('(') {
        eat_whitespace(&mut cursor);
        Some(')')
    } else {
        None
    };

    let task_id = parse_quoted_string(&mut cursor)?;
    eat_whitespace(&mut cursor);

    // If the value was wrapped in a list or tuple, consume the closing bracket.
    if let Some(bracket) = closing_bracket {
        // Allow optional trailing comma for tuples: `('task',)`.
        if cursor.eat_char(',') {
            eat_whitespace(&mut cursor);
        }
        if !cursor.eat_char(bracket) {
            return None;
        }
        eat_whitespace(&mut cursor);
    }

    // Allow an optional `key='return_value'` argument (the default XCom key).
    if cursor.eat_char(',') {
        eat_whitespace(&mut cursor);
        parse_key_return_value(&mut cursor)?;
        eat_whitespace(&mut cursor);
    }

    if !cursor.eat_char(')') {
        return None;
    }
    eat_whitespace(&mut cursor);

    if !cursor.eat_char2('}', '}') {
        return None;
    }
    eat_whitespace(&mut cursor);

    if !cursor.is_eof() || task_id.is_empty() {
        return None;
    }

    Some(task_id.to_string())
}

fn eat_whitespace(cursor: &mut Cursor) {
    cursor.eat_while(|c| c.is_ascii_whitespace());
}

fn parse_identifier<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    let source = cursor.as_str();
    if !cursor.eat_if(|c| c.is_ascii_alphabetic() || c == '_') {
        return None;
    }
    cursor.eat_while(|c| c.is_ascii_alphanumeric() || c == '_');
    let len = source.len() - cursor.as_str().len();
    Some(&source[..len])
}

/// Parse a quoted string delimited by single or double quotes.
///
/// Returns `None` if the string contains escape sequences (e.g. `\'`),
/// since Jinja supports them and correctly handling escaped content is
/// out of scope for this rule.
fn parse_quoted_string<'a>(cursor: &mut Cursor<'a>) -> Option<&'a str> {
    let quote_char = if cursor.eat_char('\'') {
        '\''
    } else if cursor.eat_char('"') {
        '"'
    } else {
        return None;
    };

    let remaining = cursor.as_str();
    let end = remaining.find(quote_char)?;
    let value = &remaining[..end];

    // Bail if the string contains escape sequences.
    if value.contains('\\') {
        return None;
    }

    cursor.skip_bytes(end + quote_char.len_utf8());
    Some(value)
}

/// If the cursor starts with `key='return_value'` (or `key="return_value"`),
/// consume it. Returns `None` if `key=` is present but the value is not
/// `return_value`.
fn parse_key_return_value(cursor: &mut Cursor) -> Option<()> {
    if parse_identifier(cursor)? != "key" {
        return None;
    }
    eat_whitespace(cursor);
    if !cursor.eat_char('=') {
        return None;
    }
    eat_whitespace(cursor);
    if parse_quoted_string(cursor)? != "return_value" {
        return None;
    }
    Some(())
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
    fn test_whitespace_around_dot() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti . xcom_pull(task_ids='my_task') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_whitespace_before_open_paren() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull (task_ids='my_task') }}"),
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
    fn test_rejects_multi_element_list() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['a', 'b']) }}"),
            None
        );
    }

    #[test]
    fn test_single_element_list() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['my_task']) }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_single_element_tuple() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=('my_task',)) }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_single_element_tuple_no_trailing_comma() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=('my_task')) }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_key_return_value() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task', key='return_value') }}"),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_key_return_value_double_quotes() {
        assert_eq!(
            parse_xcom_pull_template(
                r#"{{ ti.xcom_pull(task_ids='my_task', key="return_value") }}"#
            ),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_list_with_key_return_value() {
        assert_eq!(
            parse_xcom_pull_template(
                "{{ ti.xcom_pull(task_ids=['my_task'], key='return_value') }}"
            ),
            Some("my_task".to_string())
        );
    }

    #[test]
    fn test_rejects_non_default_key() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids='my_task', key='custom_key') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_list_with_non_default_key() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(task_ids=['my_task'], key='custom_key') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_escaped_quotes() {
        assert_eq!(
            parse_xcom_pull_template(r"{{ ti.xcom_pull(task_ids='my\_task') }}"),
            None
        );
    }

    #[test]
    fn test_rejects_unknown_keyword() {
        assert_eq!(
            parse_xcom_pull_template("{{ ti.xcom_pull(dag_id='my_task') }}"),
            None
        );
    }
}
